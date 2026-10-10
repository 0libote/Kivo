# Windows installer artwork

The NSIS installer uses the Kivo palette and `src-tauri/icons/app-icon.svg` mark.
`windows/header.svg` and `windows/sidebar.svg` are editable sources for the
150 × 57 and 164 × 314 24-bit BMP exports. The native wizard provides controls,
installation progress, and accessibility.

Exports are committed; builds require no artwork tools. Export at the source
dimensions and save bitmaps as RGB (24-bit). Keep sources and exports together.
Run `bun run check:packaging` to validate paths and dimensions. The app icons
are explicitly listed in `bundle.icon`. Stable releases remain drafts until reviewed.
