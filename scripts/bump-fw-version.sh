#!/usr/bin/env bash
# Bump a dongle firmware version, the firmware counterpart of
# scripts/bump-version.sh. The crate version is what the firmware reports over
# USB, and the release workflows refuse a tag that doesn't match it.
#
#   lite    crates/dongle-lite-fw (Lite and Pro), tag dongle-lite-vX.Y.Z
#   pro-v2  crates/dongle-pro-v2-fw (Pro v2), tag dongle-pro-v2-vX.Y.Z
#
# Usage: scripts/bump-fw-version.sh [patch|minor|major] [lite|pro-v2]
#        (defaults: patch, lite)
set -euo pipefail

cd "$(dirname "$0")/.."

kind="${1:-patch}"
case "$kind" in
  patch|minor|major) ;;
  *) echo "usage: $0 [patch|minor|major] [lite|pro-v2]" >&2; exit 1 ;;
esac
case "${2:-lite}" in
  lite) crate=crates/dongle-lite-fw tag=dongle-lite-v ;;
  pro-v2) crate=crates/dongle-pro-v2-fw tag=dongle-pro-v2-v ;;
  *) echo "usage: $0 [patch|minor|major] [lite|pro-v2]" >&2; exit 1 ;;
esac

toml=$crate/Cargo.toml
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

echo "  updating $crate/Cargo.lock"
(cd "$crate" && cargo update --workspace --quiet)

msg="chore(release): bump ${crate#crates/} to $new"
git commit --quiet -m "$msg" -- "$toml" "$crate/Cargo.lock"
echo "  committed: $msg"

echo
echo "done. next:"
echo "  git tag $tag$new && git push origin main $tag$new"
