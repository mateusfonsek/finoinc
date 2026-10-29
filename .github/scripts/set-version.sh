#!/usr/bin/env bash
# Writes the version to the THREE files where it lives, plus Cargo.lock.
#
# Exists as a single point because they already diverged by hand: Cargo.toml
# stayed at 0.1.0 while package.json and tauri.conf.json moved to 0.2.0.
set -euo pipefail

v=${1:?usage: set-version.sh <version>}

# Validates the input version before any substitution. Without this, a leading
# `v` (the most common way to pass a git tag by mistake) or an incomplete "X.Y"
# slips through node and silently produces invalid JSON. Rejecting here, loud
# and clear, is the only defence.
if [[ ! $v =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  printf 'error: invalid version: "%s" (expected X.Y.Z, without a leading "v")\n' "$v" >&2
  exit 1
fi

# Replaces only the "version" field value with a text regex, without
# JSON.parse + stringify: re-serializing reformats the whole file (tauri.conf.json
# has arrays written on a single line, such as ["app", "dmg"], and Node's
# stringify breaks each item onto its own line — dozens of diff lines because of
# ONE version).
#
# Each substitution is guarded to make sure the key was found: if `replace`
# changes nothing, the file comes out untouched and the wrong version silently
# stays in place. The same risk perl prevents with the `$done` flag below —
# leaving it uncovered here would repeat the very same bug.
node -e '
  const fs = require("fs");
  const v = process.argv[1];
  for (const f of ["package.json", "src-tauri/tauri.conf.json"]) {
    const content = fs.readFileSync(f, "utf8");
    const updated = content.replace(/"version":\s*"[^"]*"/, `"version": "${v}"`);
    if (updated === content) {
      console.error(`error: "version" key not found in ${f}`);
      process.exit(1);
    }
    fs.writeFileSync(f, updated);
  }
' "$v"

# Only the FIRST occurrence, which is the one in the [package] section (the
# file's first). Never a dependency's `version`.
#
# The flag is set only when the substitution REALLY happens — an
# `unless $done++` would set it on line 1 (`[package]`) already and the file
# would come out untouched, silently.
perl -pi -e 'if (!$done && s/^version = "[^"]*"/version = "'"$v"'"/) { $done = 1 }' src-tauri/Cargo.toml

# Same guard as the node block above, for the same reason: `perl -pi` exits 0
# even when it substitutes NOTHING (the version line only has to be indented or
# written differently). Without this check the script would finish successfully
# having bumped just the two JSON files — recreating exactly the divergence
# between the three files it exists to prevent.
#
# Anchored at the START of the line (`^`) rather than a loose search:
# `version = "1.0"` also appears inside dependency lines (`serde = { version =
# "1.0", ... }`), which come indented or after `{`. Anchoring at `^` is enough
# to discard those — a loose search would false-positive precisely when `$v`
# happened to match some dependency's version.
#
# But we do NOT use `-x` (whole line exact): the real perl above only swaps the
# quoted value, so a trailing comment (`version = "0.2.0"  # bumped by CI`),
# leftover whitespace or CRLF survive the substitution and would make the guard
# report failure on a swap that actually worked. `[[:space:]]*(#.*)?$` tolerates
# that without giving up the `^version = "..."` anchor, which is what actually
# matters here.
if ! grep -Eq '^version = "'"$v"'"[[:space:]]*(#.*)?$' src-tauri/Cargo.toml; then
  printf 'error: version %s was not written to src-tauri/Cargo.toml (the [package] `version = "..."` line was not found)\n' "$v" >&2
  exit 1
fi

# Reading the manifest already rewrites the lock with the new version.
cargo metadata --manifest-path src-tauri/Cargo.toml --format-version 1 >/dev/null
