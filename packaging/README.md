# Reference installer artwork

These assets belong to the previous Tauri NSIS installer. Native releases currently ship `kivo-native.exe` directly and do not use this artwork.

The sources use the Kivo palette and `src-tauri/icons/app-icon.svg` mark.
`windows/header.svg` and `windows/sidebar.svg` are editable sources for the 150 × 57 and 164 × 314 BMP exports. Keep the sources and 24-bit RGB exports together if installer packaging is restored.

The previous Bun packaging check and frontend build scripts have been removed. The remaining Tauri Windows bundle configuration lives in `src-tauri/tauri.windows.conf.json`.
