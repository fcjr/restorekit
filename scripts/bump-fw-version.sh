#!/usr/bin/env bash
# Bump the dongle firmware version (crates/dongle-lite-fw), the firmware
# counterpart of scripts/bump-version.sh. The crate version is what the
# firmware reports over USB, and release-fw.yml refuses a tag that doesn't
# match it.
#
# Usage: scripts/bump-fw-version.sh [patch|minor|major]   (default: patch)
set -euo pipefail

cd "$(dirname "$0")/.."

kind="${1:-patch}"
case "$kind" in
  patch|minor|major) ;;
  *) echo "usage: $0 [patch|minor|major]" >&2; exit 1 ;;
esac

toml=crates/dongle-lite-fw/Cargo.toml
current=$(perl -ne 'print $1 and exit if /^version = "([^"]+)"$/' "$toml")
if [[ ! "$current" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "error: could not parse version from $toml (got '$current')" >&2
  exit 1
fi
major="${BASH_REMATCH[1]}" minor="${BASH_REMATCH[2]}" patch="${BASH_REMATCH[3]}"

case "$kind" in
  patch) new="$major.$minor.$((patch + 1))" ;;
  minor) new="$major.$((minor + 1)).0" ;;
  major) new="$((major + 1)).0.0" ;;
esac

echo "bumping firmware $kind: $current -> $new"

CUR="$current" NEW="$new" perl -pi -e 's/^version = "\Q$ENV{CUR}\E"$/version = "$ENV{NEW}"/' "$toml"
if ! grep -qF "version = \"$new\"" "$toml"; then
  echo "error: no substitution made in $toml" >&2
  exit 1
fi
echo "  updated $toml"

echo "  updating crates/dongle-lite-fw/Cargo.lock"
(cd crates/dongle-lite-fw && cargo update --workspace --quiet)

git commit --quiet -m "chore(release): bump dongle firmware to $new" \
  -- "$toml" crates/dongle-lite-fw/Cargo.lock
echo "  committed: chore(release): bump dongle firmware to $new"

echo
echo "done. next:"
echo "  git tag dongle-lite-fw-v$new && git push origin main dongle-lite-fw-v$new"
