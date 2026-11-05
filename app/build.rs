//! Compiles the native macOS interface (app/macos, in Swift) into a static library linked into
//! the app.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let sources: Vec<PathBuf> = std::fs::read_dir("macos")
        .expect("app/macos")
        .map(|entry| entry.expect("a file").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "swift"))
        .collect();
    for source in &sources {
        println!("cargo:rerun-if-changed={}", source.display());
    }
    println!("cargo:rerun-if-changed=macos/Bridge.h");
    // A file added or removed changes the folder.
    println!("cargo:rerun-if-changed=macos");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let library = out.join("libArbaleteUI.a");
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        _ => "x86_64",
    };
    let status = Command::new("xcrun")
        .args([
            "swiftc",
            "-emit-library",
            "-static",
            "-parse-as-library",
            "-O",
        ])
        .args(["-module-name", "ArbaleteUI", "-target"])
        .arg(format!("{arch}-apple-macos14.0"))
        .args(["-import-objc-header", "macos/Bridge.h"])
        .arg("-o")
        .arg(&library)
        .args(&sources)
        .status()
        .expect("swiftc");
    assert!(status.success(), "swiftc failed");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=ArbaleteUI");
    // The Swift runtime ships with macOS; the SDK holds what links to it.
    let sdk = Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
        .expect("xcrun");
    let sdk = String::from_utf8(sdk.stdout).expect("a path");
    println!(
        "cargo:rustc-link-search=native={}/usr/lib/swift",
        sdk.trim()
    );
    println!("cargo:rustc-link-search=native=/usr/lib/swift");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    println!("cargo:rustc-link-lib=framework=SwiftUI");
    // The media viewer's videos (macos/MediaViewer.swift).
    println!("cargo:rustc-link-lib=framework=AVKit");
    // The certificate sheet (showCertificate in macos/Panels.swift).
    println!("cargo:rustc-link-lib=framework=SecurityInterface");
}
