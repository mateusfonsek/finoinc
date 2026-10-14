<sub><a href="CONTRIBUTING.pt-BR.md">🇧🇷 Leia em português</a></sub>

# Contributing

## Commit format

The repo uses [conventional commits](https://www.conventionalcommits.org/). It's not
a style preference: **the commit message is what decides the next release's
version**. CI rejects any PR whose title or commits stray from the format.

```
type(optional-scope): description
```

| Type | Effect on version | Example |
|---|---|---|
| `feat` | minor — `0.2.0` → `0.3.0` | `feat(watch): watched folder` |
| `fix` | patch — `0.2.0` → `0.2.1` | `fix: fix the SETTLE guard` |
| `perf` | patch | `perf: scan outside the mutex` |
| `i18n` | patch | `i18n: translate the language picker` |
| `docs`, `chore`, `test`, `ci`, `refactor`, `build`, `style`, `revert` | none | `docs: tweak README` |

Breaking change: `!` after the type, or `BREAKING CHANGE:` in the body. Bumps
**major** (`0.2.0` → `1.0.0`).

```
feat!: remove the legacy OFX parser
```

## How a release happens

There is no manual step. Merging into `main` is what ships:

1. You open the PR → CI runs types, tests, build and validates the commit format.
2. You merge → CI runs the suite again, computes the new version from the
   commits, writes it into the three version files, builds the universal
   `.dmg`, creates the tag and **publishes the release**.
3. If the commits are only `docs`/`chore`/`test`/`ci`/`refactor`, no release is
   created — and that's the correct behavior, not a failure.

The `.dmg` comes out at `https://github.com/MateusFonseK/finoinc/releases/latest`.

## Running locally

```sh
pnpm install
pnpm tauri dev          # development
pnpm check              # types
pnpm test               # frontend tests
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml contract::   # validates every locale pack
.github/scripts/next-version.test.sh          # CI script tests
.github/scripts/check-commit-format.test.sh
.github/scripts/release-notes.test.sh
.github/scripts/set-version.test.sh
```
