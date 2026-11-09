//! Compiles the native macOS interface (app/macos, in Swift) into a static library linked into
//! the app.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let mut sources: Vec<PathBuf> = fs::read_dir("macos")
        .expect("app/macos")
        .map(|entry| entry.expect("a file").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "swift"))
        .collect();
    sources.sort();
    for source in &sources {
        println!("cargo:rerun-if-changed={}", source.display());
    }
    println!("cargo:rerun-if-changed=macos/Bridge.h");
    // A file added or removed changes the folder.
    println!("cargo:rerun-if-changed=macos");
    // The updater, Sparkle (macos/Updates.swift): mise run sparkle downloads it.
    println!("cargo:rerun-if-env-changed=SPARKLE_PATH");
    let sparkle = env::var("SPARKLE_PATH").expect("SPARKLE_PATH, set by mise (.config/mise.toml)");
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        _ => "x86_64",
    };
    let mut args: Vec<String> = [
        "-emit-library",
        "-static",
        "-parse-as-library",
        "-O",
        "-module-name",
        "ArbaleteUI",
        "-target",
        &format!("{arch}-apple-macos14.0"),
        "-import-objc-header",
        "macos/Bridge.h",
        "-F",
        &sparkle,
    ]
    .map(String::from)
    .into();
    args.extend(sources.iter().map(|source| source.display().to_string()));

    // Clippy builds the app's build script apart from cargo build and cargo test, so each would
    // compile the same interface: the library is kept once per profile, named by what makes it.
    let mut hash = DefaultHasher::new();
    args.hash(&mut hash);
    for file in sources.iter().chain([&PathBuf::from("macos/Bridge.h")]) {
        fs::read(file).expect("a source").hash(&mut hash);
    }
    swiftc(&["--version"]).stdout.hash(&mut hash);
    let hash = format!("{:016x}", hash.finish());
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let profile_dir = out.ancestors().nth(3).expect("target/<profile>");
    let dir = profile_dir.join("arbalete-ui");
    let stamp = dir.join("hash");
    if fs::read_to_string(&stamp).ok().as_deref() != Some(hash.as_str()) {
        fs::create_dir_all(&dir).expect("target/<profile>/arbalete-ui");
        let _ = fs::remove_file(&stamp);
        // The driver runs its compile jobs one at a time unless told otherwise.
        let jobs = env::var("NUM_JOBS").expect("NUM_JOBS");
        let library = dir.join("libArbaleteUI.a").display().to_string();
        args.extend(["-j".into(), jobs, "-o".into(), library]);
        assert!(swiftc(&args).status.success(), "swiftc failed");
        fs::write(&stamp, &hash).expect("the library's hash");
    }
    println!("cargo:rustc-link-search=native={}", dir.display());
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
    // The downloads' thumbnails (FileThumbnail in macos/Library.swift).
    println!("cargo:rustc-link-lib=framework=QuickLookThumbnailing");
    // The Boosts' code editor, Monaco in a web view (macos/BoostEditor.swift).
    println!("cargo:rustc-link-lib=framework=WebKit");
    // The certificate sheet (showCertificate in macos/Panels.swift).
    println!("cargo:rustc-link-lib=framework=SecurityInterface");
    // Sparkle ships in the bundle's Frameworks. Weak, since the helpers and the tests, which
    // never update, run without it.
    println!("cargo:rustc-link-search=framework={sparkle}");
    println!("cargo:rustc-link-arg=-Wl,-weak_framework,Sparkle");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
}

/// Runs swiftc, its messages in the build's output.
fn swiftc(args: &[impl AsRef<OsStr>]) -> Output {
    Command::new("xcrun")
        .arg("swiftc")
        .args(args)
        .stderr(Stdio::inherit())
        .output()
        .expect("swiftc")
}
