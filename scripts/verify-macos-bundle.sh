#!/bin/bash
# Run on the macOS builder before publishing. Never modify the signed app.
set -euo pipefail

bundle_root="${1:?usage: verify-macos-bundle.sh <release/bundle>}"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/kivo-dmg.XXXXXX")"
mount_point="$work_dir/mount"
mkdir "$mount_point"
mounted=false
cleanup() {
  if [[ "$mounted" = true ]]; then
    hdiutil detach "$mount_point" -quiet
  fi
  rm -f "$work_dir/entitlements.plist"
  rmdir "$mount_point" "$work_dir"
}
trap cleanup EXIT

app="$bundle_root/macos/Kivo.app"
icon_name="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIconFile' "$app/Contents/Info.plist")"
[[ -f "$app/Contents/Resources/$icon_name" ]]
codesign --verify --deep --strict --verbose=2 "$app"
signature="$(codesign -dv --verbose=4 "$app" 2>&1)"
codesign --display --entitlements - --xml "$app" >"$work_dir/entitlements.plist" 2>/dev/null
library_exception="$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.cs.disable-library-validation' "$work_dir/entitlements.plist" 2>/dev/null || true)"
[[ "$library_exception" != true ]]
executable="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")"
if otool -L "$app/Contents/MacOS/$executable" | grep -q 'libKivoVozBridge'; then
  echo "The speech bridge must be embedded, not loaded from a separate dylib." >&2
  exit 1
fi
case "$signature" in
  *"Authority=Developer ID Application"*)
    # A configured paid identity must actually pass Gatekeeper, rather than
    # shipping a signed but unnotarized build with the same approval friction.
    spctl --assess --type execute --verbose=2 "$app"
    xcrun stapler validate "$app"
    ;;
esac

shopt -s nullglob
images=("$bundle_root"/dmg/*.dmg)
if [[ "${#images[@]}" -ne 1 ]]; then
  echo "Expected exactly one macOS installer." >&2
  exit 1
fi
hdiutil attach "${images[0]}" -readonly -nobrowse -mountpoint "$mount_point" -quiet
mounted=true
[[ -f "$mount_point/.DS_Store" ]]
[[ -f "$mount_point/.background/background.png" ]]
cmp "$mount_point/.background/background.png" "$(dirname "$0")/../packaging/macos/background.png"
[[ -L "$mount_point/Applications" ]]
[[ "$(readlink "$mount_point/Applications")" = /Applications ]]
codesign --verify --deep --strict --verbose=2 "$mount_point/Kivo.app"
bash "$(dirname "$0")/smoke-macos-bundle.sh" "$mount_point/Kivo.app"
echo "Verified app icon, signatures, Finder layout, branded background, Applications link and startup."
