#[cfg(windows)]
use std::{env, path::PathBuf};

fn main() {
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
