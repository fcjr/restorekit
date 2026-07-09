#!/usr/bin/env bash
# Build a signed + notarized RestoreKit.app locally, reading signing/notarization
# config from apps/desktop/.env (gitignored). See .env.example for the keys.
#
# To sign without notarizing (faster), comment out the APPLE_API_* lines in .env.
set -euo pipefail
cd "$(dirname "$0")"

if [ ! -f .env ]; then
  echo "no .env found — copy .env.example to .env and fill it in" >&2
  exit 1
fi

set -a
# shellcheck disable=SC1091
. ./.env
set +a

# Resolve a relative notarization key path against this directory so .env can
# carry a repo-relative path (e.g. acskey.p8) instead of a machine-specific
# absolute one. tauri/notarytool run from a different CWD and need it absolute.
if [ -n "${APPLE_API_KEY_PATH:-}" ]; then
  case "$APPLE_API_KEY_PATH" in
    /*) ;;
    *) export APPLE_API_KEY_PATH="$PWD/$APPLE_API_KEY_PATH" ;;
  esac
fi

# Updater artifact signing is only needed for the auto-update feed (CI produces
# the release). It needs the Tauri minisign key and its password. Sign when both
# are available (key defaults to the standard ~/.tauri location); otherwise skip
# updater artifacts so a local signed + notarized app/dmg still builds.
UPDATER_ARGS=""
TAURI_KEY="${TAURI_SIGNING_PRIVATE_KEY:-$HOME/.tauri/restorekit.key}"
if [ -f "$TAURI_KEY" ] && [ -n "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ]; then
  export TAURI_SIGNING_PRIVATE_KEY="$TAURI_KEY"
else
  echo "note: no updater signing password set — building without updater artifacts" >&2
  # The `pnpm tauri` wrapper re-joins argv through a shell, which would strip the
  # quotes from inline --config JSON, so pass the override as a temp file path.
  NO_UPDATER_DIR="$(mktemp -d)"
  trap 'rm -rf "$NO_UPDATER_DIR"' EXIT
  printf '{"bundle":{"createUpdaterArtifacts":false}}' > "$NO_UPDATER_DIR/no-updater.json"
  UPDATER_ARGS="--config $NO_UPDATER_DIR/no-updater.json"
fi

bash ./stage-helper.sh
# UPDATER_ARGS is intentionally unquoted so it splits into --config + path (which
# has no spaces) when set, and vanishes when empty.
pnpm tauri build ${UPDATER_ARGS}
