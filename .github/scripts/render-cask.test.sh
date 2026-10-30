#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")"

pass=0
fail=0

check() {
  desc=$1
  shift
  if "$@"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FAILED: %s\n' "$desc"
  fi
}

sha=$(printf 'a%.0s' $(seq 1 64))
template=../homebrew/finoinc.rb
out=$(./render-cask.sh 1.2.3 "$sha" "$template")

check "version line"           grep -qxF '  version "1.2.3"' <<<"$out"
check "sha256 line"            grep -qxF "  sha256 \"$sha\"" <<<"$out"
check "url points at the dmg"  grep -qF 'releases/download/v#{version}/Finoinc_#{version}_universal.dmg' <<<"$out"
check "no placeholder left"    bash -c '! grep -q "@[A-Z0-9]*@" <<<"$1"' _ "$out"
check "rejects v-prefixed version" sh -c '! ./render-cask.sh v1.2 "$1" "$2" >/dev/null 2>&1' _ "$sha" "$template"
check "rejects a short sha"    sh -c '! ./render-cask.sh 1.2.3 abc "$1" >/dev/null 2>&1' _ "$template"

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
