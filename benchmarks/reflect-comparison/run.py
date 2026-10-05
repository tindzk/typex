"""Measure resolved dependencies, clean builds and borrowed reflection operations."""

import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import shutil
import tempfile
import time


def run(arguments, **kwargs):
    return subprocess.run(arguments, check=True, text=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-runs", type=int, default=3)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--cpu", type=int, help="Pin runtime measurements to this Linux CPU")
    options = parser.parse_args()
    os.environ["RUSTC_WRAPPER"] = ""
    os.environ["RUSTC_WORKSPACE_WRAPPER"] = ""
    directory = Path(__file__).resolve().parent
    variants = {"typex": "compare-typex", "bevy": "compare-bevy", "bevy-minimal": "compare-bevy-minimal", "facet": "compare-facet"}
    results = {"rustc": run(["rustc", "--version"], capture_output=True).stdout.strip(), "jobs": options.jobs, "cpu": options.cpu, "variants": {}}
    for variant in variants:
        run(["cargo", "fetch", "--manifest-path", str(directory / variant / "Cargo.toml")])
    with tempfile.TemporaryDirectory(prefix="typex-reflection-comparison-") as scratch:
        for variant, binary_name in variants.items():
            manifest = str(directory / variant / "Cargo.toml")
            tree = run(["cargo", "tree", "--manifest-path", manifest, "--locked", "--offline", "-e", "normal,build", "--prefix", "none", "--format", "{p}"], capture_output=True).stdout
            packages = sorted({line.removesuffix(" (*)") for line in tree.splitlines() if line})
            result = {"dependencies": len(packages) - 2, "packages": packages, "build_seconds": {}, "runtime_ns": {}}
            results["variants"][variant] = result
            for profile in ["dev", "release"]:
                timings = []
                for trial in range(options.build_runs):
                    target = str(Path(scratch) / f"{variant}-{profile}-{trial}")
                    command = ["cargo", "build", "--manifest-path", manifest, "--locked", "--offline", "--target-dir", target, "-j", str(options.jobs)]
                    if profile == "release":
                        command.append("--release")
                    print(f"Building {variant}, {profile}, trial {trial + 1}", flush=True)
                    start = time.perf_counter()
                    run(command)
                    timings.append(time.perf_counter() - start)
                    if profile != "release" or trial != options.build_runs - 1:
                        shutil.rmtree(target)
                result["build_seconds"][profile] = {"samples": timings, "median": statistics.median(timings)}
                if profile == "release":
                    executable = str(Path(target) / "release" / binary_name)
                    command = [executable] if options.cpu is None else ["taskset", "-c", str(options.cpu), executable]
                    for trial in range(3):
                        output = run(command, capture_output=True).stdout
                        for line in output.splitlines():
                            name, median, minimum, maximum = line.split("\t")
                            result["runtime_ns"].setdefault(name, []).append({"median": float(median), "min": float(minimum), "max": float(maximum)})
                    shutil.rmtree(target)
            print(json.dumps({variant: result}, indent=2), flush=True)
    print("RESULTS_JSON", flush=True)
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
