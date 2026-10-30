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

# A release directory as the publish job sees it: each updater package next to
# its signature.
fixture() {
  dir=$(mktemp -d)
  for f in Finoinc_1.2.3_universal.app.tar.gz Finoinc_1.2.3_x64-setup.exe Finoinc_1.2.3_amd64.AppImage; do
    : > "$dir/$f"
    printf 'sig-of-%s' "$f" > "$dir/$f.sig"
  done
  printf '%s' "$dir"
}

base=https://github.com/MateusFonseK/finoinc/releases/download/v1.2.3

dir=$(fixture)
out=$(./latest-json.sh 1.2.3 "$dir")
field() { node -e 'const j = JSON.parse(process.argv[1]); console.log(eval("j" + process.argv[2]))' "$out" "$1"; }

check "parses as JSON"                 node -e 'JSON.parse(process.argv[1])' "$out"
check "carries the version"            test "$(field .version)" = "1.2.3"
check "Apple Silicon gets the universal tarball" test "$(field '["platforms"]["darwin-aarch64"].url')" = "$base/Finoinc_1.2.3_universal.app.tar.gz"
check "Intel gets the same tarball"    test "$(field '["platforms"]["darwin-x86_64"].url')" = "$base/Finoinc_1.2.3_universal.app.tar.gz"
check "Windows gets the installer"     test "$(field '["platforms"]["windows-x86_64"].url')" = "$base/Finoinc_1.2.3_x64-setup.exe"
check "Linux gets the AppImage"        test "$(field '["platforms"]["linux-x86_64"].url')" = "$base/Finoinc_1.2.3_amd64.AppImage"
check "signature is the .sig content"  test "$(field '["platforms"]["windows-x86_64"].signature')" = "sig-of-Finoinc_1.2.3_x64-setup.exe"

rm "$dir/Finoinc_1.2.3_amd64.AppImage.sig"
err=$(./latest-json.sh 1.2.3 "$dir" 2>&1 >/dev/null)
code=$?
check "missing signature fails"        test "$code" -ne 0
check "error names the missing file"   grep -qF "Finoinc_1.2.3_amd64.AppImage.sig" <<<"$err"

check "rejects a version with a v"     sh -c '! ./latest-json.sh v1.2.3 "$1" >/dev/null 2>&1' _ "$(fixture)"

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
