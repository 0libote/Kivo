# Installer artwork

The installers use the same restrained graphite palette as `src/theme/kivo.ts`
and the existing `src-tauri/icons/app-icon.svg` mark. These are static packaging
assets, separate from the application UI.

- `macos/background.svg` is the editable source for the 660 × 440 PNG. Finder
  draws the actual app and Applications icons at the positions in
  `src-tauri/tauri.conf.json`; the background deliberately leaves these areas
  empty. Open the installed copy from Applications before granting permissions.
- `windows/header.svg` and `windows/sidebar.svg` are the editable sources for
  the 150 × 57 and 164 × 314 **24-bit BMP** exports used by NSIS. The native
  wizard provides the controls, installation progress and accessibility.

The exports are committed: builds need no artwork tools, extra downloads,
subscriptions or regeneration step. To edit artwork, use an SVG editor, export
at the source dimensions, and save Windows bitmaps as RGB (24-bit). Keep the
exports and their sources together. Run `bun run check:packaging` to validate
the configured asset paths and export dimensions. The macOS workflow also
mounts the finished DMG and verifies its contents and app signatures before
beta publication; stable releases stay drafts until reviewed.
