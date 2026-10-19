#!/usr/bin/env bash
set -uo pipefail

# Tests for set-version.sh
# Creates a temporary directory with a fixture, runs the script, and checks the
# results.

script_dir="$(cd "$(dirname "$0")" && pwd)"
set_version_script="$script_dir/set-version.sh"

pass=0
fail=0

# Creates a temp directory with the file structure.
setup_fixture() {
  tmpdir=$(mktemp -d)
  mkdir -p "$tmpdir/src-tauri/src"

  # package.json
  cat > "$tmpdir/package.json" << 'EOF'
{
  "name": "finoinc",
  "version": "0.2.0",
  "type": "module"
}
EOF

  # src-tauri/tauri.conf.json (arrays on a single line, as in the real repo)
  cat > "$tmpdir/src-tauri/tauri.conf.json" << 'EOF'
{
  "build": {
    "beforeDevCommand": "",
    "devUrl": "http://localhost:5173",
    "frontendDist": "dist"
  },
  "app": {
    "windows": [{"fullscreen": false, "height": 600, "resizable": true, "title": "finoinc", "width": 800}],
    "security": {"csp": null},
    "version": "0.2.0"
  }
}
EOF

  # src-tauri/src/main.rs - minimal file so cargo metadata succeeds
  cat > "$tmpdir/src-tauri/src/main.rs" << 'EOF'
fn main() {}
EOF

  # src-tauri/Cargo.toml com [package] e [dependencies] sections
  cat > "$tmpdir/src-tauri/Cargo.toml" << 'EOF'
[package]
name = "finoinc"
version = "0.2.0"
description = "A tauri app"
authors = ["you"]
license = ""
repository = ""
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.35" }

[dependencies.tauri]
version = "1.4"
features = [ "shell-open" ]
EOF

  echo "$tmpdir"
}

# Cleans up the temp directory.
cleanup_fixture() {
  rm -rf "$1"
}

# Test 1: a valid version (1.0.0) rewrites the three files, one line each
{
  desc="versão válida (1.0.0) reescreve arquivos"
  fixture=$(setup_fixture)

  (cd "$fixture" && "$set_version_script" "1.0.0" 2>/dev/null)
  ret=$?

  if [ $ret -eq 0 ] && \
     grep -qF '"version": "1.0.0"' "$fixture/package.json" && \
     grep -qF '"version": "1.0.0"' "$fixture/src-tauri/tauri.conf.json" && \
     grep -qF 'version = "1.0.0"' "$fixture/src-tauri/Cargo.toml" && \
     grep -qF 'tokio = { version = "1.35"' "$fixture/src-tauri/Cargo.toml"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FALHOU: %s\n' "$desc"
  fi

  cleanup_fixture "$fixture"
}

# Test 2: v1.2.0 is rejected, no file modified
{
  desc="v1.2.0 é rejeitado (nenhum arquivo modificado)"
  fixture=$(setup_fixture)

  (cd "$fixture" && $set_version_script "v1.2.0" 2>/dev/null)
  ret=$?

  if [ $ret -ne 0 ] && \
     grep -qF '"version": "0.2.0"' "$fixture/package.json" && \
     grep -qF '"version": "0.2.0"' "$fixture/src-tauri/tauri.conf.json" && \
     grep -qF 'version = "0.2.0"' "$fixture/src-tauri/Cargo.toml"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FALHOU: %s\n' "$desc"
  fi

  cleanup_fixture "$fixture"
}

# Test 3: 1.2 is rejected (incomplete), no file modified
{
  desc="1.2 é rejeitado (incompleto)"
  fixture=$(setup_fixture)

  (cd "$fixture" && $set_version_script "1.2" 2>/dev/null)
  ret=$?

  if [ $ret -ne 0 ] && \
     grep -qF '"version": "0.2.0"' "$fixture/package.json" && \
     grep -qF '"version": "0.2.0"' "$fixture/src-tauri/tauri.conf.json"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FALHOU: %s\n' "$desc"
  fi

  cleanup_fixture "$fixture"
}

# Test 4: bad"version is rejected, the JSON stays valid
{
  desc='bad"version é rejeitado, JSON válido'
  fixture=$(setup_fixture)

  (cd "$fixture" && $set_version_script 'bad"version' 2>/dev/null)
  ret=$?

  # Checks that it failed
  if [ $ret -eq 0 ]; then
    fail=$((fail + 1))
    printf 'FALHOU: %s (deveria ter falhado mas teve sucesso)\n' "$desc"
  else
    # Checks that no file was modified
    if grep -qF '"version": "0.2.0"' "$fixture/package.json" && \
       grep -qF '"version": "0.2.0"' "$fixture/src-tauri/tauri.conf.json"; then
      # Checks that the JSON stayed valid
      if node -e "require('$fixture/package.json')" 2>/dev/null && \
         node -e "require('$fixture/src-tauri/tauri.conf.json')" 2>/dev/null; then
        pass=$((pass + 1))
      else
        fail=$((fail + 1))
        printf 'FALHOU: %s (JSON inválido)\n' "$desc"
      fi
    else
      fail=$((fail + 1))
      printf 'FALHOU: %s (arquivo foi modificado)\n' "$desc"
    fi
  fi

  cleanup_fixture "$fixture"
}

# Test 5: no argument is rejected
{
  desc="nenhum argumento é rejeitado"
  fixture=$(setup_fixture)

  (cd "$fixture" && $set_version_script 2>/dev/null)
  ret=$?

  if [ $ret -ne 0 ] && \
     grep -qF '"version": "0.2.0"' "$fixture/package.json"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FALHOU: %s\n' "$desc"
  fi

  cleanup_fixture "$fixture"
}

# Test 6: a valid version (1.5.3) rewrites
{
  desc="versão válida (1.5.3) reescreve arquivos"
  fixture=$(setup_fixture)

  (cd "$fixture" && $set_version_script "1.5.3" 2>/dev/null)
  ret=$?

  if [ $ret -eq 0 ] && \
     grep -qF '"version": "1.5.3"' "$fixture/package.json" && \
     grep -qF '"version": "1.5.3"' "$fixture/src-tauri/tauri.conf.json" && \
     grep -qF 'version = "1.5.3"' "$fixture/src-tauri/Cargo.toml"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FALHOU: %s\n' "$desc"
  fi

  cleanup_fixture "$fixture"
}

# Test 7: the [package] version changes, [dependencies] versions do not
{
  desc="[package] version muda, [dependencies] versões não mudam"
  fixture=$(setup_fixture)

  (cd "$fixture" && $set_version_script "2.0.0" 2>/dev/null)
  ret=$?

  if [ $ret -eq 0 ] && \
     grep -qF 'version = "2.0.0"' "$fixture/src-tauri/Cargo.toml" && \
     grep -qF 'tokio = { version = "1.35"' "$fixture/src-tauri/Cargo.toml" && \
     grep -qF 'serde = { version = "1.0"' "$fixture/src-tauri/Cargo.toml"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FALHOU: %s\n' "$desc"
  fi

  cleanup_fixture "$fixture"
}

# Test 8: a Cargo.toml whose version line does not match the perl => error
# (`^version = ` does not hit an indented line). The failure mode this test
# pins down is the SILENT one: without the guard in the script, perl substitutes
# nothing, exits 0, and the script finishes successfully having bumped only the
# JSON files.
{
  desc="Cargo.toml não substituído reprova (não fica em silêncio)"
  fixture=$(setup_fixture)

  cat > "$fixture/src-tauri/Cargo.toml" << 'EOF'
[package]
  name = "finoinc"
  version = "0.2.0"
  edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
EOF

  (cd "$fixture" && $set_version_script "3.0.0" 2>/dev/null)
  ret=$?

  if [ $ret -ne 0 ] && grep -qF 'version = "0.2.0"' "$fixture/src-tauri/Cargo.toml"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FALHOU: %s (saiu %d)\n' "$desc" "$ret"
  fi

  cleanup_fixture "$fixture"
}

# Test 9: the requested version is IDENTICAL to a dependency's, and the
# [package] line does not match the perl (indented) => it must still fail.
#
# This test exists to discriminate the guard's `^` anchor. Test 8 alone does not
# catch it: its fixture asks for "3.0.0" against a dependency at "1.0", which
# does not match by content under any comparison. Here the requested version is
# IDENTICAL to the dependency's version (`tokio = { version = "1.35.0" }`, a
# real crate version — it has to resolve for real so the `cargo metadata` at the
# end of the script is not what fails the test for an unrelated reason) — a
# guard without a start-of-line anchor (`grep -qF` instead of the current
# `grep -Eq '^version = ...'`) finds "version = \"1.35.0\"" as a SUBSTRING inside
# the dependency line and reports success, even with [package] untouched. The
# correct guard only accepts when the line STARTS with "version = ".
{
  desc="versão pedida igual à de uma dependência não engana a guarda"
  fixture=$(setup_fixture)

  cat > "$fixture/src-tauri/Cargo.toml" << 'EOF'
[package]
  name = "finoinc"
  version = "0.2.0"
  edition = "2021"

[dependencies]
tokio = { version = "1.35.0", features = ["full"] }
EOF

  (cd "$fixture" && $set_version_script "1.35.0" 2>/dev/null)
  ret=$?

  if [ $ret -ne 0 ] && grep -qF 'version = "0.2.0"' "$fixture/src-tauri/Cargo.toml"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FALHOU: %s (saiu %d)\n' "$desc" "$ret"
  fi

  cleanup_fixture "$fixture"
}

printf '\n%d passaram, %d falharam\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
