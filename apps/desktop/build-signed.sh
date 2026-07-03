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

bash ./stage-helper.sh
npm run tauri build
