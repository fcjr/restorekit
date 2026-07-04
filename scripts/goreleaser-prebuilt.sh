#!/bin/sh
# `gobinary` shim for goreleaser: the OSS distribution lacks the Pro-only
# `prebuilt` builder, so instead of compiling anything we intercept the
# `go build -o <path> ...` invocation and copy the native binary that CI
# already built for the target platform (goreleaser sets GOOS/GOARCH in the
# environment). Any other go subcommand is passed through to the real go.
set -eu

if [ "${1:-}" != "build" ]; then
    exec go "$@"
fi

out=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "-o" ]; then
        out="$arg"
    fi
    prev="$arg"
done

if [ -z "$out" ]; then
    echo "goreleaser-prebuilt.sh: no -o <output> in go build args" >&2
    exit 1
fi

ext=""
if [ "${GOOS:?}" = "windows" ]; then
    ext=".exe"
fi

cp "artifacts/restorekit_${GOOS}_${GOARCH:?}/restorekit${ext}" "$out"
