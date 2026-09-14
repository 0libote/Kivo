#!/usr/bin/env bash
#
# Kivo macOS installer (free distribution, no Apple Developer ID).
#
#   curl -fsSL https://raw.githubusercontent.com/0libote/Kivo/main/scripts/install-macos.sh | bash
#
# What this does:
#   1. Verifies Apple Silicon + macOS 26+ (the only supported beta target).
#   2. Downloads the rolling `continuous` beta .app tarball.
#   3. Extracts it, strips the browser quarantine flag that causes the
#      misleading "Kivo is damaged and can't be opened" dialog, and
#      copies Kivo.app into /Applications with ditto (preserves the
#      bundle symlinks a plain cp/tar can break).
#
# The beta is ad-hoc signed (free), not Developer-ID signed and notarized
# ($99/yr Apple Developer Program). macOS therefore still shows a one-time
# "unverified developer" approval in System Settings > Privacy & Security on
# first launch. That approval is expected and only needed once.
set -euo pipefail

REPO="0libote/Kivo"
APP_NAME="Kivo.app"
INSTALL_DIR="/Applications"
DEST="$INSTALL_DIR/$APP_NAME"

log() { printf '%s\n' "$*"; }
err() { printf 'error: %s\n' "$*" >&2; exit 1; }

# --- Preconditions -----------------------------------------------------------
[[ "$(uname -s)" == "Darwin" ]] || err "this installer is macOS-only."
[[ "$(uname -m)" == "arm64" ]] || err "Apple Silicon (arm64) is required; Intel Macs are not supported by this beta."
command -v curl >/dev/null || err "curl is required."
command -v ditto >/dev/null || err "ditto is required (ships with macOS)."

# macOS 26+ required (SpeechAnalyzer/DictationTranscriber + bundle floor).
os_version="$(sw_vers -productVersion)"
major="$(printf '%s' "$os_version" | cut -d. -f1)"
[[ "$major" -ge 26 ]] 2>/dev/null || err "macOS 26 or later is required (found $os_version)."

# --- Resolve the current beta asset ------------------------------------------
log "Resolving the latest beta..."
manifest_url="https://github.com/$REPO/releases/download/continuous/continuous.json"
manifest="$(curl -fsSL "$manifest_url")" || err "could not download $manifest_url"
version="$(printf '%s' "$manifest" | grep -o '"version"[[:space:]]*:[[:space:]]*"[^"]*"' | head -n 1 | sed -E 's/.*"([^"]+)".*/\1/')"
[[ -n "$version" ]] || err "could not parse a version from continuous.json"
asset="Kivo_${version}_aarch64.app.tar.gz"
url="https://github.com/$REPO/releases/download/continuous/$asset"
log "Installing Kivo $version (beta)..."

# --- Download + install -------------------------------------------------------
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsSL --progress-bar "$url" -o "$tmp/kivo.tgz" || err "download failed: $url"
tar -xzf "$tmp/kivo.tgz" -C "$tmp" || err "could not extract $asset"
app_path="$(find "$tmp" -maxdepth 2 -name "$APP_NAME" -type d | head -n 1)"
[[ -n "$app_path" ]] || err "no $APP_NAME found inside $asset"

# Strip quarantine BEFORE the first launch: this is what turns the dead-end
# "damaged, move to the Trash" dialog into a normal first launch. The app is
# not actually damaged; the flag just forces a full Gatekeeper assessment of
# an ad-hoc-signed (free) build.
xattr -dr com.apple.quarantine "$app_path" 2>/dev/null || true

if [[ -w "$INSTALL_DIR" ]]; then
  ditto "$app_path" "$DEST"
else
  log "Requesting permission to write to $INSTALL_DIR..."
  sudo ditto "$app_path" "$DEST"
fi
# Belt and braces: copying can re-apply quarantine on some systems.
xattr -dr com.apple.quarantine "$DEST" 2>/dev/null || true

log "Installed at $DEST"
log "First launch: open Kivo from Applications. If macOS shows an unverified-developer"
log "prompt, approve it once in System Settings > Privacy & Security."
