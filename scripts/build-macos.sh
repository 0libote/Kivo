#!/bin/bash
# Tauri distinguishes unset Apple credentials from empty strings. GitHub
# supplies empty strings for absent secrets, so remove those before bundling.
set -euo pipefail

for key in APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID; do
  if [[ -z "${!key:-}" ]]; then
    unset "$key"
  fi
done

export APPLE_SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:--}"
exec bun tauri "$@"
