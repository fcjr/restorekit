#!/usr/bin/env bash
# Build the privileged DFU helper and stage it as a Tauri externalBin sidecar,
# named with the host target triple (what Tauri expects). Run before
# `npm run tauri dev` / `npm run tauri build`.
set -euo pipefail
cd "$(dirname "$0")"

triple="$(rustc -Vv | awk '/^host:/ {print $2}')"
cargo build --release -p helper --manifest-path ../../Cargo.toml

# Tauri names sidecars with the target triple and the host's executable
# extension — `.exe` on Windows, none elsewhere.
ext=""
case "$triple" in *windows*) ext=".exe" ;; esac

mkdir -p src-tauri/binaries
cp "../../target/release/helper${ext}" \
  "src-tauri/binaries/helper-${triple}${ext}"
echo "staged helper-${triple}${ext}"
