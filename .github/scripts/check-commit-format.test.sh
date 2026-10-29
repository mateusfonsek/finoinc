#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")"

pass=0
fail=0

# `check <desc> <expected:ok|error> <pr-title> <commit-subjects>`
check() {
  desc=$1
  expected=$2
  title=$3
  subjects=$4

  if printf '%b' "$subjects" | PR_TITLE="$title" ./check-commit-format.sh >/dev/null 2>&1; then
    got=ok
  else
    got=error
  fi

  if [ "$got" = "$expected" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FAILED: %s\n  expected: %s\n  got:      %s\n' "$desc" "$expected" "$got"
  fi
}

check "valid title and commits"         ok    'feat: x'          'feat: x\n'
check "type with a digit (i18n) is valid" ok  'i18n: translate'  'i18n: translate\n'
check "scope is valid"                  ok    'feat(watch): x'   'feat(watch): x\n'
check "breaking is valid"               ok    'feat!: x'         'feat!: x\n'
check "scope + breaking is valid"       ok    'fix(ofx)!: x'     'fix(ofx)!: x\n'
check "title without a type fails"      error 'fixed stuff'      'feat: x\n'
check "title without a description fails" error 'feat:'          'feat: x\n'
check "title with an uppercase type fails" error 'Feat: x'       'feat: x\n'
check "invalid commit fails"            error 'feat: x'          'fixed stuff\n'
check "one invalid commit in the middle fails" error 'feat: x'   'feat: a\njunk\nfix: b\n'
check "merge commit is ignored"         ok    'feat: x'          'Merge branch main into feat/x\nfeat: a\n'
check "Merge pull request is ignored"   ok    'feat: x'          'Merge pull request #12 from user/branch\nfeat: a\n'
check "Merge remote-tracking is ignored" ok   'feat: x'          'Merge remote-tracking branch '\''origin/main'\''\nfeat: a\n'
check "malformed Merge fails"           error 'feat: x'          'Merge stuff without a proper type\n'
check "empty line is ignored"           ok    'feat: x'          'feat: a\n\n'
check "no commits, only a valid title"  ok    'feat: x'          ''

# Unknown type: matches the SHAPE but is not a real type. Without this check
# the PR goes green and the merge generates no release at all.
check "type typo in the title fails"    error 'feet: add watched folder' 'feat: x\n'
check "type typo in a commit fails"     error 'feat: x'          'feet: add watched folder\n'
check "made-up type fails"              error 'feat: x'          'wip: thing\n'
check "typo with a scope fails"         error 'fixx(ofx): x'     'feat: x\n'

# Every accepted type keeps passing.
check "type feat accepted"              ok    'feat: x'          'feat: x\n'
check "type fix accepted"               ok    'fix: x'           'fix: x\n'
check "type perf accepted"              ok    'perf: x'          'perf: x\n'
check "type i18n accepted"              ok    'i18n: x'          'i18n: x\n'
check "type docs accepted"              ok    'docs: x'          'docs: x\n'
check "type chore accepted"             ok    'chore: x'         'chore: x\n'
check "type test accepted"              ok    'test: x'          'test: x\n'
check "type ci accepted"                ok    'ci: x'            'ci: x\n'
check "type refactor accepted"          ok    'refactor: x'      'refactor: x\n'
check "type build accepted"             ok    'build: x'         'build: x\n'
check "type style accepted"             ok    'style: x'         'style: x\n'
check "type revert accepted"            ok    'revert: x'        'revert: x\n'

# Tests for a missing or empty PR_TITLE (they need special handling)
if printf 'feat: x\n' | ./check-commit-format.sh >/dev/null 2>&1; then
  got=ok
else
  got=error
fi
if [ "$got" = "error" ]; then
  pass=$((pass + 1))
else
  fail=$((fail + 1))
  printf 'FAILED: unset PR_TITLE fails\n  expected: error\n  got:      %s\n' "$got"
fi

if printf 'feat: x\n' | PR_TITLE="" ./check-commit-format.sh >/dev/null 2>&1; then
  got=ok
else
  got=error
fi
if [ "$got" = "error" ]; then
  pass=$((pass + 1))
else
  fail=$((fail + 1))
  printf 'FAILED: empty PR_TITLE fails\n  expected: error\n  got:      %s\n' "$got"
fi

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
