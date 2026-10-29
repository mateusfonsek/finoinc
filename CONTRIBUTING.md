<sub><a href="CONTRIBUTING.pt-BR.md">🇧🇷 Leia em português</a></sub>

# Contributing

## Where to start

- **Bring your country.** A [locale pack](locales/README.md) is plain JSON — currency,
  categories, classification rules, UI strings — and needs no Rust or Svelte.
- **Pick a small task.** Issues labelled
  [`good first issue`](https://github.com/MateusFonseK/finoinc/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
  are scoped to be done without knowing the whole codebase.
- **Something bigger?** Open an issue first, so the approach is agreed before you write it.

Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md). Security
problems go through [SECURITY.md](SECURITY.md), never a public issue.

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

## Architecture

The diagram and the main design decisions are in the
[README](README.md#-under-the-hood). What you need to know to change things:

| You want to… | Where |
|---|---|
| Change how a statement is read | `src/lib/ofx/` — parsing, encoding, normalization, reversal pairing. OFX is parsed in the frontend; only rows reach Rust |
| Add a backend feature | a module in `src-tauri/src/commands/`, registered in `collect_commands!` in `src-tauri/src/lib.rs` |
| Call it from the UI | a typed wrapper in `src/lib/api/` over `src/lib/bindings.ts` |
| Add a screen | `src/routes/` and `src/routes/routes.ts`; components live in `src/lib/components/<feature>/` |
| Change the schema | a new numbered file in `src-tauri/migrations/`, listed in `src-tauri/src/db/migrations.rs` |
| Add UI text | a key in **every** `locales/*/strings.json` |
| Touch the MCP server | `src-tauri/src/mcp/` — `protocol.rs` (HTTP + JSON-RPC), `tools/read.rs`, `tools/write.rs` |

Rules that keep the codebase coherent:

- **`src/lib/bindings.ts` is generated — never edit it by hand.** A debug build
  (`pnpm tauri dev`) rewrites it from the Rust command signatures; commit the
  regenerated file with your change.
- **Migrations are forward-only and never seed rows.** Never edit a migration that
  already shipped; write a new one. Default categories and rules come from the
  locale pack.
- **Money is never a float.** Amounts travel as strings and are parsed into `Decimal`.
- **Rules point at a category `key`, never at its name.**
- **No new network request unless it is opt-in** and listed in the README privacy table.
- Code, comments and commit messages are in English.

## Debugging

> ⚠️ `pnpm tauri dev` uses the **same database** as the installed app:
> `~/Library/Application Support/app.finoinc/finoinc.db`. If you also use Finoinc
> for your own money, back it up first (**Settings → Backup**) or copy the file.

- **Frontend:** in a dev build, right-click the window → **Inspect Element** for the
  WebKit inspector (console, network, DOM).
- **Rust:** panics and errors print in the terminal that runs `pnpm tauri dev`.
- **Database:** open the file above with `sqlite3` or any SQLite client. Quit the
  app first if you are going to write to it.
- **Import problems:** add the statement as a fixture in `src/lib/ofx/__fixtures__/`
  (with made-up data) and reproduce it in a `*.test.ts` — faster than clicking.
- **MCP server:** it listens on `127.0.0.1:7717` (or a random port if that one is
  taken — the MCP screen shows which). The same screen lists every call the agent
  makes while the app is open.
- **Single test:** `pnpm vitest run src/lib/ofx/parse.test.ts` or
  `cargo test --manifest-path src-tauri/Cargo.toml mcp::protocol`.
