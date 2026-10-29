#!/usr/bin/env bash
# Computes the next version from commit messages in the conventional commits
# format.
#
# Reads from STANDARD INPUT instead of calling `git` internally: that way the
# tests feed fixed text, with no need to build a fixture repository per case.
# The workflow is what runs `git log`, piping the two together.
#
# The input is ONE RECORD PER COMMIT, NUL-separated (\0) — not line-separated.
# A commit body is free text: a pasted changelog line, a "fix: revisit later"
# bullet, or any lowercase label followed by a colon in the middle of the body
# matches the header regex if we read line by line without splitting per commit
# first. With NUL, each `read` takes a whole commit; only the FIRST line of that
# block is treated as the header, and the rest is merely scanned for the
# BREAKING CHANGE marker. That is what guarantees a body never becomes a header
# — do not simplify this back to line-by-line, it is exactly the bug this split
# fixes.
#
#   git log --format='%B%x00' v0.2.0..HEAD | next-version.sh 0.2.0
#
# Prints the new version, or NOTHING when no commit is releasable — that is what
# keeps a docs-only merge from becoming a release.
set -euo pipefail

current=${1:?usage: next-version.sh <current-version>}

# Validates the input version before any computation. Without this, a leading
# `v` (the most common way to pass a git tag by mistake) or an incomplete "X.Y"
# slip through the `read` below and silently produce a wrong version — or a
# stray error on stderr that nobody checks. Rejecting here, loud and clear, is
# the only defence.
if [[ ! $current =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  printf 'error: invalid current version: "%s" (expected X.Y.Z, without a leading "v")\n' "$current" >&2
  exit 1
fi

IFS=. read -r major minor patch <<< "$current"

# Precedence: major > minor > patch > none. Never regresses.
bump=none

# `-d ''` makes `read` stop at NUL instead of newline: that is what separates
# one commit from the next. The `|| [ -n "$record" ]` covers input that does not
# end with NUL (last record without a terminator) — the same trick once used
# with lines, now per record. Works on bash 3.2 (no mapfile/readarray/declare -A).
while IFS= read -r -d '' record || [ -n "$record" ]; do
  # `git log --format='%B%x00'` is tformat: since the format does not end in a
  # "visible" newline (it ends at the NUL), git inserts an extra \n after each
  # entry as a record terminator. That makes every record but the first arrive
  # here with one leading newline too many. Without stripping it, the first
  # "real" line falls into the body and the header comes out empty — every
  # commit from the second on would be ignored. Strips only that newline (a
  # no-op when absent, as in the first record).
  record=${record#$'\n'}

  # The record's first line is the commit header; the rest is body/footer,
  # where we only look for the breaking-change marker.
  header=${record%%$'\n'*}
  if [ "$header" = "$record" ]; then
    body=
  else
    body=${record#*$'\n'}
  fi

  # Conventional header: type(optional scope)(optional !): description.
  # The type accepts a digit because `i18n` is a real type in this repo — with
  # `[a-z]+` the validator would reject commits already on main.
  [[ $header =~ ^([a-z][a-z0-9]*)(\([^\)]*\))?(!)?:[[:space:]] ]] || continue

  type=${BASH_REMATCH[1]}
  breaking=${BASH_REMATCH[3]}

  # BREAKING CHANGE can be on any line of the body/footer, on its own.
  #
  # The scan comes AFTER header validation on purpose: the footer only has
  # meaning inside a conventional commit. Scanning first let a non-conforming
  # commit ("fixed some stuff") whose body quoted the marker — in a pasted
  # changelog, in a release note — bump the major all by itself. It is the same
  # class of bug the NUL split closed for headers, left open for the footer.
  while IFS= read -r body_line || [ -n "$body_line" ]; do
    if [[ $body_line =~ ^BREAKING[[:space:]-]CHANGE: ]]; then
      bump=major
    fi
  done <<< "$body"

  if [ -n "$breaking" ]; then
    bump=major
  elif [ "$type" = feat ] && [ "$bump" != major ]; then
    bump=minor
  elif [ "$bump" = none ]; then
    case $type in
      fix | perf | i18n) bump=patch ;;
    esac
  fi
done

case $bump in
  major) printf '%d.0.0\n' "$((major + 1))" ;;
  minor) printf '%d.%d.0\n' "$major" "$((minor + 1))" ;;
  patch) printf '%d.%d.%d\n' "$major" "$minor" "$((patch + 1))" ;;
  none) : ;; # no output = no release
esac
