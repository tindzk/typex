#!/usr/bin/env python3
"""Compile and execute the Rust examples embedded in README.md."""

from __future__ import annotations

import difflib
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
README = ROOT / "README.md"
CHECK_ROOT = ROOT / ".readme-check"
SAMPLE_MARKER = re.compile(r"<!--\s*check:\s+(.+?)\s*-->")
FENCE = re.compile(r"```(rust|text)\s*$")
IDENTIFIER = re.compile(r"^[a-z0-9-]+$")
PRELUDE = """#![allow(dead_code, unused_imports, unused_mut, unused_variables)]

use typex::{
  AccessKind, ValueKind, FieldPath, FieldPathMut, Meta, MetaMut,
  Object, ObjectOps, ObjectRefMut, PathSegment, SendObject, TypeInfo, TypeMap,
};
"""


@dataclass(frozen=True)
class Sample:
  name: str
  parent: str | None
  source: str


@dataclass(frozen=True)
class ExpectedOutput:
  displayed: str


@dataclass(frozen=True)
class TokeiCheck:
  paths: tuple[Path, ...]
  excludes: tuple[str, ...]
  minimum: int
  maximum: int


class CheckError(Exception):
  """A README validation error suitable for a concise command-line report."""


def parse_marker(line: str, line_number: int) -> tuple[str, dict[str, str]] | None:
  match = SAMPLE_MARKER.fullmatch(line.strip())
  if not match:
    return None

  attributes: dict[str, str] = {}
  for token in match.group(1).split():
    if "=" not in token:
      raise CheckError(f"line {line_number}: malformed check attribute {token!r}")
    key, value = token.split("=", 1)
    if key in attributes:
      raise CheckError(f"line {line_number}: duplicate check attribute {key!r}")
    attributes[key] = value

  if "name" in attributes:
    return "sample", attributes
  if "output" in attributes:
    return "output", attributes
  if "tokei" in attributes:
    return "tokei", attributes
  raise CheckError(f"line {line_number}: check marker needs name=, output= or tokei=")


def parse_readme() -> tuple[dict[str, Sample], dict[str, ExpectedOutput], list[TokeiCheck]]:
  samples: dict[str, Sample] = {}
  outputs: dict[str, ExpectedOutput] = {}
  tokei_checks: list[TokeiCheck] = []
  pending: tuple[str, dict[str, str], int] | None = None
  active_language: str | None = None
  active_lines: list[str] = []
  active_name: str | None = None

  for line_number, raw_line in enumerate(README.read_text(encoding="utf-8").splitlines(True), 1):
    line = raw_line.rstrip("\r\n")

    if active_language is not None:
      if line.strip() == "```":
        content = "".join(active_lines)
        if active_language == "rust":
          assert active_name is not None
          samples[active_name] = Sample(
            active_name,
            pending[1].get("extends") if pending else None,
            content,
          )
        else:
          assert active_name is not None
          outputs[active_name] = ExpectedOutput(content)
        active_language = None
        active_lines = []
        active_name = None
        pending = None
      else:
        active_lines.append(raw_line)
      continue

    marker = parse_marker(line, line_number)
    if marker is not None:
      if pending is not None:
        raise CheckError(f"line {line_number}: check marker has no code fence")
      marker_kind, attributes = marker
      if marker_kind == "tokei":
        if set(attributes) not in (
          {"tokei", "min", "max"},
          {"tokei", "exclude", "min", "max"},
        ):
          raise CheckError(
            f"line {line_number}: tokei marker needs tokei=, min= and max=, with optional exclude="
          )
        paths = tuple(Path(path) for path in attributes["tokei"].split(","))
        if not paths or any(not path.parts for path in paths):
          raise CheckError(f"line {line_number}: tokei marker needs at least one path")
        if any(path.is_absolute() or ".." in path.parts for path in paths):
          raise CheckError(f"line {line_number}: tokei paths must stay inside the repository")
        try:
          minimum = int(attributes["min"])
        except ValueError as error:
          raise CheckError(f"line {line_number}: tokei min must be an integer") from error
        if minimum < 0:
          raise CheckError(f"line {line_number}: tokei min must not be negative")
        try:
          maximum = int(attributes["max"])
        except ValueError as error:
          raise CheckError(f"line {line_number}: tokei max must be an integer") from error
        if maximum < 0:
          raise CheckError(f"line {line_number}: tokei max must not be negative")
        if minimum > maximum:
          raise CheckError(f"line {line_number}: tokei min must not exceed max")
        excludes = tuple(
          pattern for pattern in attributes.get("exclude", "").split(",") if pattern
        )
        tokei_checks.append(TokeiCheck(paths, excludes, minimum, maximum))
        continue
      pending = (marker[0], marker[1], line_number)
      continue

    fence = FENCE.fullmatch(line.strip())
    if fence is None:
      continue

    language = fence.group(1)
    if pending is None:
      raise CheckError(f"line {line_number}: {language} fence has no check marker")

    marker_kind, attributes, marker_line = pending
    if language == "rust" and marker_kind != "sample":
      raise CheckError(f"line {line_number}: Rust fence uses output marker from line {marker_line}")
    if language == "text" and marker_kind != "output":
      raise CheckError(f"line {line_number}: text fence uses sample marker from line {marker_line}")

    name = attributes["name"] if marker_kind == "sample" else attributes["output"]
    if not IDENTIFIER.fullmatch(name):
      raise CheckError(f"line {marker_line}: invalid sample ID {name!r}")
    if marker_kind == "sample" and name in samples:
      raise CheckError(f"line {marker_line}: duplicate sample {name!r}")
    if marker_kind == "output" and name in outputs:
      raise CheckError(f"line {marker_line}: duplicate output {name!r}")

    active_language = language
    active_name = name

  if active_language is not None:
    raise CheckError("README contains an unterminated code fence")
  if pending is not None:
    raise CheckError(f"line {pending[2]}: check marker has no code fence")
  if not samples:
    raise CheckError("README contains no marked Rust samples")

  for sample in samples.values():
    if sample.parent is not None and sample.parent not in samples:
      raise CheckError(f"sample {sample.name!r} extends unknown sample {sample.parent!r}")
  for output in outputs:
    if output not in samples:
      raise CheckError(f"output {output!r} refers to an unknown sample")

  return samples, outputs, tokei_checks


def sample_source(sample: Sample, samples: dict[str, Sample], visiting: set[str]) -> str:
  if sample.name in visiting:
    raise CheckError(f"circular sample dependency at {sample.name!r}")
  visiting.add(sample.name)
  source = ""
  if sample.parent is not None:
    source = sample_source(samples[sample.parent], samples, visiting)
  visiting.remove(sample.name)
  return source + sample.source


def write_if_changed(path: Path, content: str) -> None:
  if path.is_file() and path.read_text(encoding="utf-8") == content:
    return
  path.write_text(content, encoding="utf-8")


def check_tokei_counts(tokei_checks: list[TokeiCheck]) -> None:
  for check in tokei_checks:
    missing = [
      path
      for path in check.paths
      if not (ROOT / path).is_file() and not (ROOT / path).is_dir()
    ]
    if missing:
      raise CheckError(f"tokei source path does not exist: {missing[0]}")
    try:
      result = subprocess.run(
        [
          "tokei",
          "--output",
          "json",
          *(argument for pattern in check.excludes for argument in ("--exclude", pattern)),
          *(str(path) for path in check.paths),
        ],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
      )
    except FileNotFoundError as error:
      raise CheckError("tokei is required for README line-count checks") from error
    except subprocess.CalledProcessError as error:
      detail = error.stderr.strip() or "unknown error"
      raise CheckError(f"tokei failed: {detail}") from error
    try:
      count = json.loads(result.stdout)["Rust"]["code"]
    except (KeyError, TypeError, json.JSONDecodeError) as error:
      raise CheckError("tokei returned no Rust code count") from error
    if count < check.minimum or count > check.maximum:
      raise CheckError(
        f"tokei counted {count} Rust code lines in {', '.join(map(str, check.paths))}, "
        f"expected between {check.minimum} and {check.maximum}"
      )


def run_check() -> None:
  samples, outputs, tokei_checks = parse_readme()
  check_tokei_counts(tokei_checks)

  bin_path = CHECK_ROOT / "src" / "bin"
  bin_path.mkdir(parents=True, exist_ok=True)
  cargo_manifest = CHECK_ROOT / "Cargo.toml"
  write_if_changed(
    cargo_manifest,
    """[workspace]

[package]
name = "typex-readme-check"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
typex = { path = %s, features = ["derive"] }
"""
    % json.dumps(str(ROOT / "typex")),
  )

  expected_sources = {f"readme-{name}.rs" for name in samples}
  for source in bin_path.glob("readme-*.rs"):
    if source.name not in expected_sources:
      source.unlink()

  for sample in samples.values():
    source = bin_path / f"readme-{sample.name}.rs"
    write_if_changed(
      source,
      PRELUDE + "\nfn main() {\n" + sample_source(sample, samples, set()) + "}\n",
    )

  subprocess.run(
    ["cargo", "build", "--manifest-path", str(cargo_manifest), "--bins", "--quiet"],
    cwd=ROOT,
    check=True,
  )

  for sample in samples.values():
    executable = CHECK_ROOT / "target" / "debug" / f"readme-{sample.name}"
    result = subprocess.run([str(executable)], cwd=ROOT, check=True, capture_output=True, text=True)
    expected_output = outputs.get(sample.name)
    if expected_output is None and result.stdout:
      raise CheckError(f"sample {sample.name!r} produced unexpected output:\n{result.stdout}")
    if expected_output is not None and result.stdout != expected_output.displayed:
      diff = "".join(
        difflib.unified_diff(
          expected_output.displayed.splitlines(True),
          result.stdout.splitlines(True),
          fromfile=f"README expected output ({sample.name})",
          tofile=f"sample output ({sample.name})",
        )
      )
      raise CheckError(diff)


def main() -> int:
  try:
    run_check()
  except CheckError as error:
    print(f"README check failed: {error}", file=sys.stderr)
    return 1
  print("README Rust samples compiled and produced expected output.")
  return 0


if __name__ == "__main__":
  raise SystemExit(main())
