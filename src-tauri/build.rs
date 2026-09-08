use std::{env, path::PathBuf, process::Command};

fn main() {
    #[cfg(target_os = "macos")]
    build_speech_bridge();
    tauri_build::build();
}

#[cfg(target_os = "macos")]
fn build_speech_bridge() {
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let source = PathBuf::from("native/macos/SpeechBridge.swift");
    let library = output.join("libKivoSpeech.a");
    println!("cargo:rerun-if-changed={}", source.display());

    let status = Command::new("xcrun")
        .args([
            "swiftc",
            "-parse-as-library",
            "-emit-library",
            "-static",
            "-O",
            "-module-name",
            "KivoSpeech",
        ])
        .arg(&source)
        .arg("-o")
        .arg(&library)
        .status()
        .expect("xcrun must be available to build Kivo on macOS");
    assert!(
        status.success(),
        "the native Speech bridge failed to compile"
    );

    let swiftc = Command::new("xcrun")
        .args(["--find", "swiftc"])
        .output()
        .expect("xcrun could not locate swiftc");
    let swiftc = PathBuf::from(
        String::from_utf8(swiftc.stdout)
            .expect("swiftc path is UTF-8")
            .trim(),
    );
    let toolchain_swift = swiftc
        .parent()
        .and_then(|path| path.parent())
        .expect("swiftc is inside a toolchain")
        .join("lib/swift/macosx");
    let sdk = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .expect("xcrun could not locate the macOS SDK");
    let sdk_swift = PathBuf::from(
        String::from_utf8(sdk.stdout)
            .expect("SDK path is UTF-8")
            .trim(),
    )
    .join("usr/lib/swift");

    println!("cargo:rustc-link-search=native={}", output.display());
    println!(
        "cargo:rustc-link-search=native={}",
        toolchain_swift.display()
    );
    println!("cargo:rustc-link-search=native={}", sdk_swift.display());
    println!("cargo:rustc-link-lib=static=KivoSpeech");
    println!("cargo:rustc-link-lib=framework=Speech");
    println!("cargo:rustc-link-lib=framework=AVFoundation");
    println!("cargo:rustc-link-lib=framework=CoreMedia");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
}
