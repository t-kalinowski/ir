#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

tmp="${TMPDIR:-/tmp}/ir-r-versions-$$.json"
trap 'rm -f "$tmp"' EXIT INT HUP TERM

case "$(uname -s)" in
  Darwin)
    platform="macos"
    ;;
  Linux)
    # shellcheck disable=SC1091
    . /etc/os-release
    platform="linux-${ID}-${VERSION_ID}"
    ;;
  MINGW*|MSYS*|CYGWIN*|Windows_NT)
    platform="windows"
    ;;
  *)
    echo "unsupported platform: $(uname -s)" >&2
    exit 1
    ;;
esac

arch="$(uname -m)"
case "${platform}/${arch}" in
  macos/aarch64|macos/arm64)
    arch="arm64"
    ;;
  */arm64)
    arch="aarch64"
    ;;
esac

rig available --json --all --platform "$platform" --arch "$arch" > "$tmp"
mv "$tmp" src/rig/r-versions.json
printf '%s/%s\n' "$platform" "$arch" > src/rig/r-versions-target.txt
date -u +%F > src/rig/r-versions-fetched-at.txt
