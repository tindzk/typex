use std::{env, process::Command};

fn main() {
  println!("cargo::rerun-if-changed=build.rs");
  println!("cargo::rerun-if-env-changed=RUSTC");
  println!("cargo::rustc-check-cfg=cfg(typex_trait_upcasting)");

  let compiler = env::var_os("RUSTC").expect("Cargo must provide RUSTC");
  let output = Command::new(compiler)
    .arg("--version")
    .output()
    .expect("failed to query the Rust compiler version");
  assert!(
    output.status.success(),
    "Rust compiler version query failed"
  );
  let output = String::from_utf8(output.stdout).expect("Rust compiler version must be UTF-8");
  let version = output
    .split_whitespace()
    .nth(1)
    .expect("Rust compiler version must include a release number");
  let mut components = version.split('.');
  let major = components
    .next()
    .and_then(|value| value.parse::<u32>().ok())
    .expect("Rust compiler version must include a major number");
  let minor = components
    .next()
    .and_then(|value| value.parse::<u32>().ok())
    .expect("Rust compiler version must include a minor number");

  if (major, minor) >= (1, 86) {
    println!("cargo::rustc-cfg=typex_trait_upcasting");
  }
}
