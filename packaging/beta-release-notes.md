> HISTORICAL / UNUSED: this note described a rolling beta channel (a `continuous`
> pre-release, `-setup.exe` installers, `.sig`/`continuous.json` in-app updates)
> that was never shipped. Native releases publish portable executables to
> versioned draft releases instead, and no `publish-continuous` workflow job
> exists. Kept for reference only; the Tauri bundle config in
> `src-tauri/tauri.windows.conf.json` is untouched.

This is the latest development beta of Kivo, rebuilt from `main` after the required CI checks pass. It may include unfinished features or regressions. Prefer a stable release when one is available.

## Download and install

- **Windows:** Windows 11 24H2 or later. Download and run the `-setup.exe` installer. Windows may show a SmartScreen warning for an unsigned build.

The `.sig` and `continuous.json` files support in-app updates; use the installers above for a first installation.

## Updates and feedback

Beta installations follow this rolling channel through Settings → About when updater signing is configured. Local builds without an updater key require a manual installation.

Report problems in [GitHub Issues](https://github.com/0libote/Kivo/issues), including your OS version, Kivo version, and steps to reproduce. Leave out API keys, private text, and recordings.
