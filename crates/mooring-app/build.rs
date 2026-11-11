use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=src/native.m");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let object = out.join("native.o");
    // For the target, not the machine building it: an Intel build on Apple silicon linked an
    // arm64 bridge and failed on every kl_* symbol (2026-11-12).
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "x86_64",
        _ => "arm64",
    };
    let status = Command::new("xcrun")
        .args([
            "clang",
            "-arch",
            arch,
            "-fobjc-arc",
            "-fblocks",
            "-c",
            "src/native.m",
            "-o",
        ])
        .arg(&object)
        .status()
        .expect("compile native bridge");
    assert!(status.success(), "native bridge compilation failed");
    let status = Command::new("ar")
        .arg("crs")
        .arg(out.join("libmooring_native.a"))
        .arg(object)
        .status()
        .expect("archive native bridge");
    assert!(status.success(), "native bridge archive failed");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=mooring_native");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=UserNotifications");
}
