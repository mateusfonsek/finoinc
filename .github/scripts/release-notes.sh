#!/usr/bin/env bash
# Builds the release notes from commit subjects (stdin).
#
# Only the types that generate a bump get in. Whoever opens a release wants to
# know what changed for THEM — not that a dependency moved or a test got
# covered.
set -euo pipefail

# The input is read ONCE into the variable. Running four `grep`s straight off
# stdin does not work: the first consumes everything and the other three get
# empty input — the bug is silent, with only the first section filled in.
all=$(cat)

feats=$(printf '%s\n' "$all" | grep -E '^feat(\([^)]+\))?!?: ' || true)
fixes=$(printf '%s\n' "$all" | grep -E '^fix(\([^)]+\))?!?: ' || true)
perfs=$(printf '%s\n' "$all" | grep -E '^perf(\([^)]+\))?!?: ' || true)
i18ns=$(printf '%s\n' "$all" | grep -E '^i18n(\([^)]+\))?!?: ' || true)

section() {
  title=$1
  body=$2
  [ -z "$body" ] && return 0
  printf '### %s\n\n' "$title"
  # The scope (what sits between parentheses, as in `feat(watch):`) is kept at
  # the front of the line — only the commit type (feat/fix/perf/i18n) is noise
  # to whoever reads the release. Hence the explicit type list: a generic
  # `^[a-z][a-z0-9]*` would match the already-rewritten scope again (e.g.
  # "watch: watched folder") and erase the scope.
  printf '%s\n' "$body" | sed -E 's/^(feat|fix|perf|i18n)\(([^)]+)\)!?: /\2: /; s/^(feat|fix|perf|i18n)!?: //; s/^/- /'
  printf '\n'
}

section "New" "$feats"
section "Fixes" "$fixes"
section "Performance" "$perfs"
section "Translations" "$i18ns"

cat <<'EOF'
---

## Install (macOS)

1. Download the `.dmg` below, open it and drag **Finoinc** into **Applications**.
2. **First launch.** The app is not signed with a paid Apple Developer certificate, so macOS blocks it once with *"Apple could not verify 'Finoinc' is free of malware…"*. Click **OK** — never *Move to Trash* — then run this in Terminal:

   ```sh
   xattr -dr com.apple.quarantine "/Applications/Finoinc.app"
   ```

   Prefer not to use the Terminal? **System Settings → Privacy & Security → "Open Anyway"**, right after trying to open the app.

3. Open it normally. Everything stays **100% local**, in `~/Library/Application Support/app.finoinc/finoinc.db`.
EOF
