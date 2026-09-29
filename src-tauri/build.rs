#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(any(target_os = "macos", windows))]
use std::{env, path::PathBuf};

fn main() {
    #[cfg(target_os = "macos")]
    build_speech_bridge();
    #[cfg(windows)]
    link_vulkan_sdk();
    tauri_build::build();
}

/// transcribe-cpp-sys links `vulkan-1` by name but does not add the SDK's lib
/// directory, and the official Vulkan SDK installer sets `VULKAN_SDK` but not
/// `LIB`. Add the search path here so a Windows Vulkan build links whenever the
/// SDK is installed. CMake already fails earlier with a clear error if the SDK
/// is missing, so a missing variable here is not an error path.
#[cfg(windows)]
fn link_vulkan_sdk() {
    let Some(sdk) = env::var_os("VULKAN_SDK") else {
        return;
    };
    let lib = PathBuf::from(sdk).join("Lib");
    if lib.is_dir() {
        println!("cargo:rustc-link-search=native={}", lib.display());
    }
}

#[cfg(target_os = "macos")]
fn build_speech_bridge() {
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let package = manifest.join("native/macos/VozBridge");
    let package_build = output.join("voz-swift-build");
    let swift_arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        arch => panic!("unsupported macOS Voz target architecture: {arch:?}"),
    };
    // The bridge uses no post-15 API. Keep its deployment target at the
    // upstream SDK's macOS floor; the containing Kivo app still declares 26+.
    let swift_triple = format!("{swift_arch}-apple-macosx15.0");
    let library = package_build
        .join(&swift_triple)
        .join("release/libKivoVozBridge.dylib");
    let packaged_library = package.join("libKivoVozBridge.dylib");
    println!(
        "cargo:rerun-if-changed={}",
        package.join("Package.swift").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        package
            .join("Sources/KivoVozBridge/VozBridge.swift")
            .display()
    );

    let status = Command::new("swift")
        .args(["build", "--package-path"])
        .arg(&package)
        .args(["--build-path"])
        .arg(&package_build)
        .args(["--triple"])
        .arg(&swift_triple)
        .args(["--configuration", "release", "--product", "KivoVozBridge"])
        .status()
        .expect("Swift 6.2 must be available to build the official Voz package");
    assert!(status.success(), "the native Voz bridge failed to compile");
    let install_name_status = Command::new("install_name_tool")
        .args(["-id", "@rpath/libKivoVozBridge.dylib"])
        .arg(&library)
        .status()
        .expect("install_name_tool must be available in the macOS SDK");
    assert!(
        install_name_status.success(),
        "could not set the packaged Voz bridge runtime path"
    );
    std::fs::copy(&library, &packaged_library)
        .expect("copy the Voz bridge where Tauri can bundle and sign it");

    let speech_source = manifest.join("native/macos/SpeechBridge.swift");
    let speech_library = output.join("libKivoSpeech.a");
    println!("cargo:rerun-if-changed={}", speech_source.display());
    let speech_status = Command::new("xcrun")
        .args([
            "swiftc",
            "-parse-as-library",
            "-emit-library",
            "-static",
            "-O",
            "-module-name",
            "KivoSpeech",
        ])
        .arg(&speech_source)
        .arg("-o")
        .arg(&speech_library)
        .status()
        .expect("xcrun must be available to build Kivo's system speech bridge");
    assert!(
        speech_status.success(),
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
        package_build.join(&swift_triple).join("release").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        toolchain_swift.display()
    );
    println!("cargo:rustc-link-search=native={}", sdk_swift.display());
    println!("cargo:rustc-link-lib=dylib=KivoVozBridge");
    println!("cargo:rustc-link-lib=static=KivoSpeech");
    println!("cargo:rustc-link-lib=framework=Speech");
    println!("cargo:rustc-link-lib=framework=AVFoundation");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=CoreMedia");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
}
