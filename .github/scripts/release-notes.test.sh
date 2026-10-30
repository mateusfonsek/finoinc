#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")"

pass=0
fail=0

# `contains <desc> <must-contain|must-not-contain> <snippet> <subjects>`
contains() {
  desc=$1
  mode=$2
  needle=$3
  subjects=$4

  out=$(printf '%b' "$subjects" | ./release-notes.sh)
  if printf '%s' "$out" | grep -qF "$needle"; then
    got=must-contain
  else
    got=must-not-contain
  fi

  if [ "$got" = "$mode" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FAILED: %s\n  expected: %s [%s]\n  output:\n%s\n' "$desc" "$mode" "$needle" "$out"
  fi
}

# `exact_line <desc> <whole-line> <subjects>`
#
# Exists because `contains` is far too loose to cover the ONLY transformation
# release-notes.sh performs. Searching for "watched folder" matches equally in
# `- feat(watch): watched folder` (the raw line, with no normalisation at all),
# and searching for "watch" even matches an intact `feat(watch):`. In other
# words: the whole sed could be ripped out and the suite would stay green.
# Comparing the WHOLE LINE (`grep -x`) is what pins the final shape.
exact_line() {
  desc=$1
  line=$2
  subjects=$3

  out=$(printf '%b' "$subjects" | ./release-notes.sh)
  # `--` is mandatory: every expected line starts with the markdown hyphen, and
  # without it grep would read "- watch: ..." as an option.
  if printf '%s\n' "$out" | grep -qxF -- "$line"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FAILED: %s\n  expected the exact line: %s\n  output:\n%s\n' "$desc" "$line" "$out"
  fi
}

contains "feat becomes New"           must-contain     "### New"          'feat: watched folder\n'
contains "feat shows without the prefix" must-contain   "watched folder"   'feat: watched folder\n'
contains "fix becomes Fixes"         must-contain     "### Fixes"        'fix: fix the guard\n'
contains "perf becomes Performance"  must-contain     "### Performance"  'perf: outside the mutex\n'
contains "i18n becomes Translations" must-contain     "### Translations" 'i18n: translate\n'
contains "chore does not show"       must-not-contain "bump dependency"  'chore: bump dependency\n'
contains "docs does not show"        must-not-contain "tweak README"     'docs: tweak README\n'
contains "empty section is not printed" must-not-contain "### Fixes"     'feat: a\n'
contains "scope is preserved"        must-contain     "watch"            'feat(watch): watched folder\n'
contains "Gatekeeper block always included" must-contain "xattr -dr com.apple.quarantine" 'feat: a\n'
contains "install section always included"  must-contain "## Install" 'fix: a\n'
contains "macOS install steps"       must-contain "### macOS"              'fix: a\n'
contains "Windows install steps"     must-contain "### Windows"            'fix: a\n'
contains "SmartScreen step"          must-contain "More info → Run anyway" 'fix: a\n'
contains "Linux install steps"       must-contain "### Linux"              'fix: a\n'
contains "AppImage made executable"  must-contain "chmod +x"               'fix: a\n'
contains ".deb installed with apt"   must-contain "sudo apt install ./"    'fix: a\n'

# The line's final shape, character by character: the type goes away, the scope
# becomes a `scope: ` prefix, and the item gains the markdown hyphen.
exact_line "scope becomes a prefix, type goes away" '- watch: watched folder' 'feat(watch): watched folder\n'
exact_line "no scope, only the description" '- something else'       'feat: something else\n'
exact_line "fix with a scope likewise"      '- ofx: fix the guard'   'fix(ofx): fix the guard\n'
exact_line "breaking change loses the !"    '- new api'              'feat!: new api\n'
exact_line "breaking with a scope"          '- db: new api'          'feat(db)!: new api\n'

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
