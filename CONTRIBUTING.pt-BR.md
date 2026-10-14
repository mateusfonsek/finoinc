<sub><a href="CONTRIBUTING.md">🇺🇸 Read in English</a></sub>

# Contribuindo

## Formato de commit

O repo usa [conventional commits](https://www.conventionalcommits.org/). Não é
preferência de estilo: **a mensagem de commit é o que decide a versão da próxima
release**. O CI reprova PR cujo título ou commits fujam do formato.

```
tipo(escopo opcional): descrição
```

| Tipo | Efeito na versão | Exemplo |
|---|---|---|
| `feat` | minor — `0.2.0` → `0.3.0` | `feat(watch): pasta observada` |
| `fix` | patch — `0.2.0` → `0.2.1` | `fix: corrige guard do SETTLE` |
| `perf` | patch | `perf: varre fora do mutex` |
| `i18n` | patch | `i18n: traduz seletor de idioma` |
| `docs`, `chore`, `test`, `ci`, `refactor`, `build`, `style`, `revert` | nenhum | `docs: ajusta README` |

Mudança incompatível: `!` depois do tipo, ou `BREAKING CHANGE:` no corpo. Vai
para **major** (`0.2.0` → `1.0.0`).

```
feat!: remove o parser OFX legado
```

## Como uma release acontece

Não existe passo manual. Mergear na `main` é o que lança:

1. Você abre a PR → o CI roda tipos, testes, build e valida o formato dos commits.
2. Você mergeia → o CI roda a suíte de novo, calcula a versão nova pelos commits,
   escreve nos três arquivos de versão, builda o `.dmg` universal, cria a tag e
   **publica a release**.
3. Se os commits forem só `docs`/`chore`/`test`/`ci`/`refactor`, nenhuma release é
   criada — e isso é o comportamento correto, não uma falha.

O `.dmg` sai em `https://github.com/MateusFonseK/finoinc/releases/latest`.

## Rodando localmente

```sh
pnpm install
pnpm tauri dev          # desenvolvimento
pnpm check              # tipos
pnpm test               # testes do frontend
cargo test --manifest-path src-tauri/Cargo.toml
.github/scripts/next-version.test.sh          # testes dos scripts de CI
.github/scripts/check-commit-format.test.sh
.github/scripts/release-notes.test.sh
.github/scripts/set-version.test.sh
```
