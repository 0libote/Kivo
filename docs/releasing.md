# Releases and signing

[Back to Kivo](../README.md)

## Packaging and signing

### macOS

`bun tauri build` produces the macOS app and DMG. The bundle is ad-hoc signed by default (`signingIdentity: "-"` in `src-tauri/tauri.conf.json`, free, no certificate needed) to give the app and bundled native code complete signatures. This does not make the app Apple-verified; downloaded builds can still be blocked by Gatekeeper. The stable release workflow overrides this with a real Developer ID via `APPLE_SIGNING_IDENTITY` when the Apple signing/notarization secrets are configured. Direct distribution is required; do not enable App Sandbox or submit this build to the Mac App Store.

The Voz speech bridge is statically linked into Kivo's executable. Ad-hoc signatures have no Team ID, so shipping it as a separate dylib causes macOS library validation to reject it before startup. Embedding it keeps hardened-runtime library validation enabled for both free and Developer ID builds. The macOS packaging gate confirms there is no dynamic Voz dependency or library-validation exception, then launches the DMG-contained app and waits for startup completion in addition to checking its signatures and icon.

Beta builds are available from the [continuous releases](https://github.com/0libote/Kivo/releases/tag/continuous). Both release workflows use Developer ID signing and notarization when the Apple secrets are configured; otherwise they keep free ad-hoc signing. Updater signatures authenticate downloads but do not replace Apple's code signing.

Install from the DMG:

1. Drag Kivo onto the Applications folder in the installer window. Quit any older running copy before replacing it.
2. Eject the Kivo disk and open Kivo from **Applications**, rather than from the mounted installer or Downloads.
3. For an unnotarized build, try opening the installed copy, then use **System Settings → Privacy & Security → Open Anyway** and confirm **Open**. This is Apple's [per-app approval procedure](https://support.apple.com/en-gb/102445).
4. Grant the permissions Kivo requests during onboarding. These are separate from Gatekeeper's approval.

Approval should persist when reopening the same installed build, including after restarting the Mac. A replacement ad-hoc signed build can require approval and renewed Accessibility/Input Monitoring grants. If prompts recur without an update, confirm you are opening the same `/Applications/Kivo.app`, remove duplicate Dock/login-item references, and see the [permission troubleshooting guide](user-guide.md#macos-beta-repeated-prompts-and-settings-shows-on-app-shows-off).

If a trusted download is blocked with a “damaged” message and Open Anyway is unavailable, the targeted fallback below removes quarantine from **Kivo only**. It is not a required install step and does not grant Accessibility or other permissions:

```sh
xattr -dr com.apple.quarantine /Applications/Kivo.app
```

There is no free certificate configuration that makes an individual developer's app Apple-verified. Developer ID distribution requires [Apple Developer Program membership](https://developer.apple.com/programs/whats-included/) (currently US$99/year or local equivalent); [fee waivers](https://developer.apple.com/help/account/membership/fee-waivers/) are limited to eligible organizations. Do not disable Gatekeeper globally or reset permissions on every launch.

The branded Finder window uses committed artwork and explicit icon positions. CI sets `TAURI_BUNDLER_DMG_IGNORE_CI=true` because Tauri otherwise skips Finder customization on CI runners. Both macOS release workflows invoke `scripts/build-macos.sh`, which removes empty Apple credential variables before launching Tauri so absent secrets do not trigger certificate import or notarization. `scripts/verify-macos-bundle.sh` checks the original and DMG-contained app signatures, Finder metadata, background directory, and Applications link on the macOS runner. PR checks also build debug DMG and NSIS installers without release secrets and verify the DMG. Installer artwork sources and exports are documented in [packaging/README.md](../packaging/README.md).

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

`bun run prepare:release` creates the ignored release-only Tauri config and injects the updater public key from the environment. Normal local builds intentionally have no trusted updater key and can check availability but cannot install a signed update. Both desktops check GitHub Releases: a newer stable release takes precedence; beta installations also follow the rolling `continuous` release when stable is already current. The beta manifest identifies same-version rebuilds by commit SHA and carries signed updater archives for both platforms. An up-to-date stable installation is not offered a rolling beta. Settings → About offers **Download and Install** for available stable or beta updates, then restarts automatically to finish, with a **Restart now** fallback if the app remains open. Existing beta installations without an embedded updater public key require one manual upgrade to a signed beta build before in-app updates can work. Without Apple credentials, beta builds remain ad-hoc signed on macOS (free, not notarized), so first installation may require macOS approval and updates may require renewed permissions. Configured Apple credentials are also used for betas.
