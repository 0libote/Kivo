#!/bin/bash
# Run on the macOS builder before publishing. Never modify the signed app.
set -euo pipefail

bundle_root="${1:?usage: verify-macos-bundle.sh <release/bundle>}"
app="$bundle_root/macos/Kivo.app"
codesign --verify --deep --strict --verbose=2 "$app"
signature="$(codesign -dv --verbose=4 "$app" 2>&1)"
case "$signature" in
  *"Authority=Developer ID Application"*)
    # A configured paid identity must actually pass Gatekeeper, rather than
    # shipping a signed but unnotarized build with the same approval friction.
    spctl --assess --type execute --verbose=2 "$app"
    xcrun stapler validate "$app"
    ;;
esac

mount_point="$(mktemp -d "${TMPDIR:-/tmp}/kivo-dmg.XXXXXX")"
mounted=false
cleanup() {
  if [[ "$mounted" = true ]]; then
    hdiutil detach "$mount_point" -quiet
  fi
  rmdir "$mount_point"
}
trap cleanup EXIT

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
echo "Verified app signatures, Finder layout, branded background and Applications link."
