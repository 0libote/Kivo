# Releases and signing

[Back to Kivo](../README.md)

## Packaging and signing

### macOS

`bun tauri build` produces the macOS app and DMG. The bundle is ad-hoc signed by default (`signingIdentity: "-"` in `src-tauri/tauri.conf.json`, free, no certificate needed) so Gatekeeper shows a recoverable unverified-developer approval instead of the dead-end "damaged" dialog. The stable release workflow overrides this with a real Developer ID via `APPLE_SIGNING_IDENTITY` when the Apple signing/notarization secrets are configured. Direct distribution is required; do not enable App Sandbox or submit this build to the Mac App Store.

Beta builds are available from the [continuous releases](https://github.com/0libote/Kivo/releases/tag/continuous). Download the DMG and install it manually.

Manual DMG install: drag `Kivo.app` to `/Applications`, then run once:

```sh
xattr -dr com.apple.quarantine /Applications/Kivo.app
```

Then open Kivo and approve it in System Settings → Privacy & Security if asked. Right-click → Open no longer bypasses Gatekeeper on recent macOS, and the "damaged" wording does not mean the download is corrupt — do not trash the app.

### Windows .exe

```powershell
bun run tauri build --bundles nsis
```

The installer is written to `src-tauri/target/release/bundle/nsis/Kivo_<version>_x64-setup.exe`. It installs for the current user, appears in Start and Installed apps, and enforces Windows 11 24H2 or later. WebView2 is bootstrapped if missing. Host the installer as a public GitHub Release asset: downloading needs no account or payment.

Unsigned builds work but may receive SmartScreen warnings; free hosting does not provide trusted publisher signing. The stable workflow signs the installer when an Authenticode certificate is configured. NSIS is the only Windows packaging route.

### GitHub Releases and updates

Pushing an `app-v*` tag runs `.github/workflows/release.yml`, builds macOS and Windows artifacts, and drafts a GitHub Release. Both stable and rolling beta in-app updates use these repository secrets:

- `TAURI_UPDATER_PUBKEY`
- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- Apple signing/notarization secrets listed in the workflow

Optional Windows signing: `WINDOWS_CERTIFICATE_BASE64` and `WINDOWS_CERTIFICATE_PASSWORD`. No Windows signing secret is required to build the installer.

The updater checks `https://github.com/0libote/Kivo/releases/latest/download/latest.json`. Never commit updater private keys or signing certificates.

`bun run prepare:release` creates the ignored release-only Tauri config and injects the updater public key from the environment. Normal local builds intentionally have no trusted updater key and can check availability but cannot install a signed update. Both desktops check GitHub Releases: a newer stable release takes precedence; beta installations also follow the rolling `continuous` release when stable is already current. The beta manifest identifies same-version rebuilds by commit SHA and carries signed updater archives for both platforms. An up-to-date stable installation is not offered a rolling beta. Settings → About offers **Download and Install** for available stable or beta updates, then restarts automatically to finish, with a **Restart now** fallback if the app remains open. Existing beta installations without an embedded updater public key require one manual upgrade to a signed beta build before in-app updates can work. Beta builds remain ad-hoc signed on macOS (free, not notarized), so first installation may require the one-time macOS approval above.
