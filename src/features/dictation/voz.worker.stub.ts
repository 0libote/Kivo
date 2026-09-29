// Only the Windows Tauri build includes the Voz Web SDK worker. Other builds
// retain the lazy worker boundary without bundling its ONNX/WASM runtime.
export {};
