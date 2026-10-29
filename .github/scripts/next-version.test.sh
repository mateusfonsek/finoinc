#!/usr/bin/env bash
# Tests for the bump calculation. Pure shell on purpose: no new dependency, it
# runs the same on a laptop and in CI.
set -uo pipefail
cd "$(dirname "$0")"

pass=0
fail=0

# `input` uses literal \n and \0, expanded by printf %b. \0 separates one
# commit from the next — the format the script expects (see the comment in
# next-version.sh on why input is per record, not per line).
# `expected_exit` is checked alongside stdout: the contract promises exit 0 on
# both the releasable path and the "no release" one, and exit 1 (with empty
# stdout) when the input version is invalid.
check() {
  desc=$1
  current=$2
  expected=$3
  expected_exit=$4
  input=$5

  got=$(printf '%b' "$input" | ./next-version.sh "$current")
  status=$?

  if [ "$got" = "$expected" ] && [ "$status" -eq "$expected_exit" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FAILED: %s\n  expected: [%s] exit %d\n  got:      [%s] exit %d\n' \
      "$desc" "$expected" "$expected_exit" "$got" "$status"
  fi
}

check "docs alone does not release"     0.2.0 ""      0 'docs: tweak README\n\0'
check "chore alone does not release"    0.2.0 ""      0 'chore: bump dependency\n\0'
check "test/ci/refactor do not release" 0.2.0 ""      0 'test: cover x\n\0ci: tweak workflow\n\0refactor: extract helper\n\0'
check "empty input does not release"    0.2.0 ""      0 ''
check "feat bumps minor"                0.2.0 "0.3.0" 0 'feat: watched folder\n\0'
check "fix bumps patch"                 0.2.0 "0.2.1" 0 'fix: fix the guard\n\0'
check "perf bumps patch"                0.2.0 "0.2.1" 0 'perf: scan outside the mutex\n\0'
check "i18n bumps patch"                0.2.0 "0.2.1" 0 'i18n: translate strings\n\0'
check "scope is recognized"             0.2.0 "0.3.0" 0 'feat(watch): watched folder\n\0'
check "feat! bumps major"               0.2.0 "1.0.0" 0 'feat!: remove legacy import\n\0'
check "feat(scope)! bumps major"        0.2.0 "1.0.0" 0 'feat(ofx)!: remove old parser\n\0'
check "BREAKING CHANGE in the body bumps major" 0.2.0 "1.0.0" 0 'feat: change something\n\nBREAKING CHANGE: the format changed\n\0'
# The footer only counts inside a conventional commit: in a non-conforming
# commit the marker may be quoted text (pasted changelog, release note).
check "BREAKING CHANGE in a malformed commit is ignored" 0.2.0 ""      0 'fixed some stuff\n\nBREAKING CHANGE: quoted from a changelog\n\0'
check "malformed with marker does not taint the others"  0.2.0 "0.2.1" 0 'fixed stuff\n\nBREAKING CHANGE: quoted\n\0fix: real\n\0'
# ...but in a conventional commit of any type, the marker counts (spec).
check "BREAKING CHANGE in chore bumps major"             0.2.0 "1.0.0" 0 'chore: touch something\n\nBREAKING CHANGE: for real\n\0'
check "highest bump wins: feat + fix"      0.2.0 "0.3.0" 0 'fix: a\n\0feat: b\n\0'
check "highest bump wins: reverse order"   0.2.0 "0.3.0" 0 'feat: b\n\0fix: a\n\0'
check "highest bump wins: breaking + feat" 0.2.0 "1.0.0" 0 'feat: b\n\0feat!: c\n\0'
check "malformed commit is ignored"        0.2.0 "0.2.1" 0 'fixed some stuff\n\0fix: a\n\0'
check "only malformed does not release"    0.2.0 ""      0 'fixed some stuff\n\0'
check "starts from 0.0.0"                  0.0.0 "0.1.0" 0 'feat: first\n\0'
check "patch does not downgrade a minor already found" 0.2.0 "0.3.0" 0 'feat: a\n\0fix: b\n\0perf: c\n\0'
check "version with large numbers"         1.9.9 "1.10.0" 0 'feat: a\n\0'
check "i18n(scope) bumps patch"            0.2.0 "0.2.1" 0 'i18n(watch): x\n\0'
check "chore!: bumps major (breaking on a non-feat type)" 0.2.0 "1.0.0" 0 'chore!: x\n\0'

# --- Regression: a body line must not be mistaken for a header ---
# This is the real defect's case: a `chore:` commit whose body quotes
# "fix: fixed..." (e.g. pasted from a changelog) must not become a release.
# If the NUL/first-line split is reverted to line-by-line reading, this test
# fails (the result becomes "0.2.1").
check "body with a 'fix: something' line is not treated as a header" \
  0.2.0 "" 0 'chore: bump deps\n\nRelease notes:\nfix: fixed bug in the changelog\n\0'

check "body mentioning 'chore: something' does not downgrade feat" \
  0.2.0 "0.3.0" 0 'feat: add something\n\nchore: something\n\0'

check "BREAKING CHANGE as the last line of a multi-line body bumps major" \
  0.2.0 "1.0.0" 0 'feat: change x\n\nexplaining line\nanother line\nBREAKING CHANGE: broke everything\n\0'

check "second commit's body quoting 'feat: x' does not release (header is docs:)" \
  0.2.0 "" 0 'docs: a\n\0docs: b\n\nfeat: x\n\0'

# `git log --format='%B%x00'` is tformat: since it does not end in a "visible"
# newline (it ends at the NUL), git inserts an extra \n after each entry. That
# makes every record from the second on arrive with one leading newline too
# many — simulated here without needing a real repository.
check "record with an extra leading \\n (git tformat artifact) still recognizes the header" \
  0.2.0 "0.3.0" 0 'docs: a\n\0\nfeat: b\n\0'

# --- Input version validation ---
check "version with a leading 'v' is rejected (empty stdout, exit != 0)" \
  v1.2.0 "" 1 'feat: x\n\0'
check "incomplete version (no patch) is rejected" \
  1.2 "" 1 'feat: x\n\0'
check "non-numeric version is rejected" \
  abc "" 1 'feat: x\n\0'
check "empty version is rejected" \
  "" "" 1 'feat: x\n\0'

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
