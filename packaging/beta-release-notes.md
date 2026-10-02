This is the latest development beta of Kivo, rebuilt from `main` after the required CI checks pass. It may include unfinished features or regressions. Prefer a stable release when one is available.

## Download and install

- **macOS:** Apple Silicon, macOS 26 or later. Download the `.dmg`, open it, and drag Kivo to Applications. This beta is ad-hoc signed and not notarized. Follow the [macOS installation guide](https://github.com/0libote/Kivo/blob/main/docs/releasing.md#macos) if macOS blocks it. Accessibility and Input Monitoring permissions may need to be granted again after updates.
- **Windows:** Windows 11 24H2 or later. Download and run the `-setup.exe` installer. Windows may show a SmartScreen warning for an unsigned build.

The `.app.tar.gz`, `.sig`, and `continuous.json` files support in-app updates; use the installers above for a first installation.

## Updates and feedback

Beta installations follow this rolling channel through Settings → About when updater signing is configured. Local builds without an updater key require a manual installation.

Report problems in [GitHub Issues](https://github.com/0libote/Kivo/issues), including your OS version, Kivo version, and steps to reproduce. Leave out API keys, private text, and recordings.
