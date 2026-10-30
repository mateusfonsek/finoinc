#!/usr/bin/env bash
# Renders the Homebrew cask for one release to stdout.
set -euo pipefail

v=${1:?usage: render-cask.sh <version> <sha256> <template>}
sha=${2:?usage: render-cask.sh <version> <sha256> <template>}
template=${3:?usage: render-cask.sh <version> <sha256> <template>}

# A bad value would be pushed to the tap and break `brew install` for everyone
# until the next release, so both are checked before anything is written.
if [[ ! $v =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  printf 'error: invalid version: "%s" (expected X.Y.Z)\n' "$v" >&2
  exit 1
fi
if [[ ! $sha =~ ^[0-9a-f]{64}$ ]]; then
  printf 'error: invalid sha256: "%s"\n' "$sha" >&2
  exit 1
fi

sed -e "s/@VERSION@/$v/" -e "s/@SHA256@/$sha/" "$template"
