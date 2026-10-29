<sub><a href="CONTRIBUTING.md">🇺🇸 Read in English</a></sub>

# Contribuindo

## Por onde começar

- **Traga o seu país.** Um [locale pack](locales/README.md) é JSON puro — moeda,
  categorias, regras de classificação, textos da interface — e não exige Rust nem Svelte.
- **Pegue uma tarefa pequena.** Issues com a label
  [`good first issue`](https://github.com/MateusFonseK/finoinc/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
  foram recortadas para serem feitas sem conhecer o código inteiro.
- **Algo maior?** Abra uma issue antes, para combinar a abordagem antes de escrever.

Todo mundo que participa segue o [código de conduta](CODE_OF_CONDUCT.md). Problemas de
segurança vão pelo [SECURITY.md](SECURITY.md), nunca por issue pública.

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

## Arquitetura

O diagrama e as principais decisões de design estão no
[README](README.pt-BR.md#-por-dentro). O que você precisa saber para mexer:

| Você quer… | Onde |
|---|---|
| Mudar como um extrato é lido | `src/lib/ofx/` — parsing, encoding, normalização, pareamento de estornos. O OFX é parseado no frontend; só as linhas chegam ao Rust |
| Adicionar uma funcionalidade no backend | um módulo em `src-tauri/src/commands/`, registrado no `collect_commands!` de `src-tauri/src/lib.rs` |
| Chamar isso da interface | um wrapper tipado em `src/lib/api/` sobre `src/lib/bindings.ts` |
| Adicionar uma tela | `src/routes/` e `src/routes/routes.ts`; os componentes ficam em `src/lib/components/<feature>/` |
| Mudar o schema | um novo arquivo numerado em `src-tauri/migrations/`, listado em `src-tauri/src/db/migrations.rs` |
| Adicionar texto na interface | uma chave em **todos** os `locales/*/strings.json` |
| Mexer no servidor MCP | `src-tauri/src/mcp/` — `protocol.rs` (HTTP + JSON-RPC), `tools/read.rs`, `tools/write.rs` |

Regras que mantêm o código coerente:

- **`src/lib/bindings.ts` é gerado — nunca edite à mão.** Um build de debug
  (`pnpm tauri dev`) o reescreve a partir das assinaturas dos comandos Rust; commite o
  arquivo regenerado junto com a sua mudança.
- **Migrações só avançam e nunca inserem linhas.** Nunca edite uma migração que já foi
  lançada; escreva uma nova. Categorias e regras padrão vêm do locale pack.
- **Dinheiro nunca é float.** Valores trafegam como string e são lidos como `Decimal`.
- **Regras apontam para a `key` da categoria, nunca para o nome.**
- **Nenhuma requisição de rede nova, a não ser opcional** e listada na tabela de
  privacidade do README.
- Código, comentários e mensagens de commit são em inglês.

## Depuração

> ⚠️ `pnpm tauri dev` usa o **mesmo banco** do app instalado:
> `~/Library/Application Support/app.finoinc/finoinc.db`. Se você também usa o Finoinc
> para o seu dinheiro, faça backup antes (**Configurações → Backup**) ou copie o arquivo.

- **Frontend:** num build de dev, clique com o botão direito na janela → **Inspecionar
  Elemento** para abrir o inspetor do WebKit (console, rede, DOM).
- **Rust:** panics e erros aparecem no terminal que roda `pnpm tauri dev`.
- **Banco:** abra o arquivo acima com `sqlite3` ou qualquer cliente SQLite. Feche o app
  antes se for escrever nele.
- **Problemas de importação:** adicione o extrato como fixture em
  `src/lib/ofx/__fixtures__/` (com dados inventados) e reproduza num `*.test.ts` — é
  mais rápido que clicar.
- **Servidor MCP:** escuta em `127.0.0.1:7717` (ou numa porta aleatória se essa estiver
  ocupada — a tela do MCP mostra qual). A mesma tela lista cada chamada que o agente faz
  enquanto o app está aberto.
- **Um teste só:** `pnpm vitest run src/lib/ofx/parse.test.ts` ou
  `cargo test --manifest-path src-tauri/Cargo.toml mcp::protocol`.
