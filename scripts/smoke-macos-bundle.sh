#!/bin/bash
# Execute the packaged app: codesign verification alone cannot detect dyld
# library-validation failures. Use on the native builder, not an end user's Mac.
set -euo pipefail

app="${1:?usage: smoke-macos-bundle.sh <Kivo.app>}"
executable="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")"
startup_log="$(mktemp "${TMPDIR:-/tmp}/kivo-startup.XXXXXX")"
app_pid=""
cleanup() {
  if [[ -n "$app_pid" ]] && kill -0 "$app_pid" 2>/dev/null; then
    kill "$app_pid"
    wait "$app_pid" || true
  fi
  rm -f "$startup_log"
}
trap cleanup EXIT

"$app/Contents/MacOS/$executable" >"$startup_log" 2>&1 &
app_pid=$!
for ((attempt = 0; attempt < 30; attempt++)); do
  if ! kill -0 "$app_pid" 2>/dev/null; then
    cat "$startup_log"
    echo "Packaged Kivo exited before startup completed." >&2
    exit 1
  fi
  if grep -q '^Kivo startup complete$' "$startup_log"; then
    # Keep it alive briefly after setup to catch an immediate runtime crash.
    sleep 2
    kill -0 "$app_pid"
    echo "Packaged Kivo started successfully."
    exit 0
  fi
  sleep 1
done
cat "$startup_log"
echo "Packaged Kivo did not complete startup within 30 seconds." >&2
exit 1
