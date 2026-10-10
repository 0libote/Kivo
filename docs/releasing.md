# Releases and signing

[Back to Kivo](../README.md)

## Packaging and signing

### Windows .exe

```powershell
bun run tauri build --bundles nsis
```

The installer is written to `src-tauri/target/release/bundle/nsis/Kivo_<version>_x64-setup.exe`. It installs for the current user, appears in Start and Installed apps, and enforces Windows 11 24H2 or later. WebView2 is bootstrapped if missing. Host the installer as a public GitHub Release asset: downloading needs no account or payment.

Unsigned builds work but may receive SmartScreen warnings; free hosting does not provide trusted publisher signing. The stable workflow signs the installer when an Authenticode certificate is configured. NSIS is the only Windows packaging route.

### GitHub Releases and updates

Pushing an `app-v*` tag runs `.github/workflows/release.yml`, builds Windows artifacts, and drafts a GitHub Release. Both stable and rolling beta in-app updates use these repository secrets:

- `TAURI_UPDATER_PUBKEY`
- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

Optional Windows signing: `WINDOWS_CERTIFICATE_BASE64` and `WINDOWS_CERTIFICATE_PASSWORD`. No Windows signing secret is required to build the installer.

The updater checks `https://github.com/0libote/Kivo/releases/latest/download/latest.json`. Never commit updater private keys or signing certificates.

`bun run prepare:release` creates the ignored release-only Tauri config and injects the updater public key from the environment. Normal local builds intentionally have no trusted updater key and can check availability but cannot install a signed update. Windows checks GitHub Releases: a newer stable release takes precedence; beta installations also follow the rolling `continuous` release when stable is already current. The beta manifest identifies same-version rebuilds by commit SHA and carries signed updater archives for Windows. An up-to-date stable installation is not offered a rolling beta. Settings → About offers **Download and Install** for available stable or beta updates, then restarts automatically to finish, with a **Restart now** fallback if the app remains open. Existing beta installations without an embedded updater public key require one manual upgrade to a signed beta build before in-app updates can work.


## Native Windows release checklist

Use the PR's review installer on Windows 11 24H2+ before publishing:

- Install for the current user, launch from Start, verify tray and autostart, then uninstall.
- In Notepad and a browser text field, test Ctrl+Win tap/start/stop, hold/release, cancellation, and a custom shortcut. Confirm Kivo does not leave modifier keys held.
- Capture and rewrite selected text, replace it, and verify the clipboard is restored. Change focus or selection during generation and confirm the result is retained without inserting into the new target.
- Test system SAPI with an installed language and denied microphone access; test the local model with GPU and CPU fallback when available.
- Download, prepare, transcribe with, and remove Voz in the actual WebView2 runtime. Verify cancellation and an unsupported or mismatched language failure.
- Check overlays on mixed-DPI monitors and confirm they preserve the editor's focus.
- Exercise a signed update through Settings → About, including a same-version beta rebuild, and confirm restart loads the advertised version.

Linux simulation and Chromium browser tests cover shared behavior but cannot certify these native integrations. macOS installers are no longer produced. Old release assets remain historical downloads; no new macOS updates are published.
