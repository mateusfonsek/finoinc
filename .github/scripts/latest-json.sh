#!/usr/bin/env bash
# Writes the updater manifest (`latest.json`) for one release to stdout.
#
# Installed apps read it from releases/latest/download/latest.json. Every
# platform must be present: an entry left out does not fail anywhere — that OS
# simply never hears of the update — so a missing package or signature stops
# the release here instead.
set -euo pipefail

v=${1:?usage: latest-json.sh <version> <dir>}
dir=${2:?usage: latest-json.sh <version> <dir>}

if [[ ! $v =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  printf 'error: invalid version: "%s" (expected X.Y.Z, without a leading "v")\n' "$v" >&2
  exit 1
fi

# Stable names (no version): the website and README link to
# releases/latest/download/<name>. The version lives in the release URL.
mac="Finoinc_universal.app.tar.gz"
win="Finoinc_x64-setup.exe"
linux="Finoinc_amd64.AppImage"

for f in "$mac" "$win" "$linux"; do
  for g in "$f" "$f.sig"; do
    if [ ! -f "$dir/$g" ]; then
      printf 'error: %s is missing from %s\n' "$g" "$dir" >&2
      exit 1
    fi
  done
done

# JSON through node, never string concatenation: a signature is free text.
node -e '
  const fs = require("fs");
  const [v, dir, mac, win, linux] = process.argv.slice(1);
  const base = `https://github.com/MateusFonseK/finoinc/releases/download/v${v}`;
  const entry = (f) => ({
    signature: fs.readFileSync(`${dir}/${f}.sig`, "utf8").trim(),
    url: `${base}/${f}`,
  });
  // One universal bundle serves both Mac architectures.
  const platforms = {
    "darwin-aarch64": entry(mac),
    "darwin-x86_64": entry(mac),
    "windows-x86_64": entry(win),
    "linux-x86_64": entry(linux),
  };
  const notes = `https://github.com/MateusFonseK/finoinc/releases/tag/v${v}`;
  const out = { version: v, notes, pub_date: new Date().toISOString(), platforms };
  process.stdout.write(JSON.stringify(out, null, 2) + "\n");
' "$v" "$dir" "$mac" "$win" "$linux"
