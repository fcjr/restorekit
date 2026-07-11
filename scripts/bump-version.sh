#!/usr/bin/env bash
# Bump the project version everywhere it is pinned.
#
# Usage: scripts/bump-version.sh [patch|minor|major]   (default: patch)
#
# Updates:
#   - Cargo.toml                            [workspace.package] version
#   - crates/restorekit/Cargo.toml          restorekit-sys dep version
#   - crates/restorekit-cli/Cargo.toml      restorekit dep version
#   - apps/desktop/src-tauri/Cargo.toml     package version (outside the workspace)
#   - apps/desktop/src-tauri/tauri.conf.json
#   - apps/desktop/package.json
#   - Cargo.lock + apps/desktop/src-tauri/Cargo.lock (via cargo)
set -euo pipefail

cd "$(dirname "$0")/.."

kind="${1:-patch}"
case "$kind" in
  patch|minor|major) ;;
  *) echo "usage: $0 [patch|minor|major]" >&2; exit 1 ;;
esac

current=$(perl -ne 'print $1 and exit if /^version = "([^"]+)"$/' Cargo.toml)
if [[ ! "$current" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "error: could not parse workspace version from Cargo.toml (got '$current')" >&2
  exit 1
fi
major="${BASH_REMATCH[1]}" minor="${BASH_REMATCH[2]}" patch="${BASH_REMATCH[3]}"

case "$kind" in
  patch) new="$major.$minor.$((patch + 1))" ;;
  minor) new="$major.$((minor + 1)).0" ;;
  major) new="$((major + 1)).0.0" ;;
esac

echo "bumping $kind: $current -> $new"

# replace <file> <perl-substitution>: apply the edit and require it to have
# actually changed something.
replace() {
  local file="$1" expr="$2"
  CUR="$current" NEW="$new" perl -pi -e "$expr" "$file"
  if ! grep -qF "$new" "$file"; then
    echo "error: no substitution made in $file" >&2
    exit 1
  fi
  echo "  updated $file"
}

replace Cargo.toml \
  's/^version = "\Q$ENV{CUR}\E"$/version = "$ENV{NEW}"/'
replace crates/restorekit/Cargo.toml \
  's/^(restorekit-sys = \{.*version = ")\Q$ENV{CUR}\E(")/$1$ENV{NEW}$2/'
replace crates/restorekit/Cargo.toml \
  's/^(restorekit-dongle-proto = \{.*version = ")\Q$ENV{CUR}\E(")/$1$ENV{NEW}$2/'
replace crates/restorekit-cli/Cargo.toml \
  's/^(restorekit = \{.*version = ")\Q$ENV{CUR}\E(")/$1$ENV{NEW}$2/'
replace apps/desktop/src-tauri/Cargo.toml \
  's/^version = "\Q$ENV{CUR}\E"$/version = "$ENV{NEW}"/'
replace apps/desktop/src-tauri/tauri.conf.json \
  's/^(\s*"version": ")\Q$ENV{CUR}\E(",?)$/$1$ENV{NEW}$2/'

replace apps/desktop/package.json \
  's/^(\s*"version": ")\Q$ENV{CUR}\E(",?)$/$1$ENV{NEW}$2/'

echo "  updating Cargo.lock"
cargo update --workspace --quiet
echo "  updating apps/desktop/src-tauri/Cargo.lock"
(cd apps/desktop/src-tauri && cargo update --workspace --quiet)

files=(
  Cargo.toml Cargo.lock
  crates/restorekit/Cargo.toml crates/restorekit-cli/Cargo.toml
  apps/desktop/package.json
  apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock
  apps/desktop/src-tauri/tauri.conf.json
)
git commit --quiet -m "chore(release): bump version to $new" -- "${files[@]}"
echo "  committed: chore(release): bump version to $new"

echo
echo "done. next:"
echo "  git tag v$new && git push origin main v$new"
