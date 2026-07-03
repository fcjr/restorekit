#!/usr/bin/env bash
# Build the privileged DFU helper and stage it as a Tauri externalBin sidecar,
# named with the host target triple (what Tauri expects). Run before
# `npm run tauri dev` / `npm run tauri build`.
set -euo pipefail
cd "$(dirname "$0")"

triple="$(rustc -Vv | awk '/^host:/ {print $2}')"
cargo build --release -p helper --manifest-path ../../Cargo.toml

mkdir -p src-tauri/binaries
cp "../../target/release/helper" \
  "src-tauri/binaries/helper-${triple}"
echo "staged helper-${triple}"
