//! Records the real compiler version in `build_string()` (it used to fall back to a hard-coded "1.85",
//! so two builds from different compilers claimed to be the same build; spec P-7 "same build").
use std::process::Command;

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let v = Command::new(rustc)
        .arg("-V")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.split_whitespace().nth(1).map(String::from))
        .unwrap_or_else(|| "unknown".into());
    if std::env::var("AEOI_RUSTC_VERSION").is_err() {
        println!("cargo:rustc-env=AEOI_RUSTC_VERSION={v}");
    }
    println!("cargo:rerun-if-env-changed=AEOI_RUSTC_VERSION");
    println!("cargo:rerun-if-env-changed=RUSTC");
}
