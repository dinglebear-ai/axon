# Axon — Repository Agent Instructions
Last verified against the repository: 2026-09-27

`AGENTS.md` is the canonical instruction file. `CLAUDE.md` and `GEMINI.md`
are direct relative symlinks to it, not independent copies. Scoped
`AGENTS.md` files extend these rules for their directories; preserve that
scope when editing. Change the canonical file, never replace an alias.

Keep shared instructions environment-agnostic. Machine names, private endpoints,
absolute checkout paths, hardware assignments, and personal workflow settings
belong in a local, Git-ignored `AGENTS.override.md`, never this file. Codex
selects that override instead of the sibling `AGENTS.md`; a local override
must explicitly direct the agent to read this shared guide first. The local
Claude alias is `CLAUDE.local.md -> AGENTS.override.md`; both remain untracked.
See the documentation maintenance guide for the verified discovery rules.

Start with this guide, the relevant scoped guide, and the implementation.
Use [the documentation index](docs/README.md) to navigate and
[the documentation maintenance guide](docs/development/documentation.md)
for authority, generated files, and lightweight validation. Dated plans and
session reports record past decisions; they do not override current code.

Unified source acquisition, document preparation, indexing, retrieval, and RAG
in one Rust binary backed by SQLite, Qdrant, TEI, Chrome/CDP, and a configured
LLM provider.

## Repository Facts

| Fact | Source of truth |
|---|---|
| Remote / default branch | `git@github.com:dinglebear-ai/axon.git`, `main` |
| Workspace membership | Root `axon`, `xtask`, `xtask-release`, and the crates listed in `Cargo.toml` `[workspace].members`; the palette has a separate workspace |
| Edition / minimum Rust | `Cargo.toml` `[workspace.package]` |
| Build toolchain | `rust-toolchain.toml`; check for `RUSTUP_TOOLCHAIN` overrides before building |
| Product version | `Cargo.toml` `[package]` and `[workspace.package]`, synchronized by release tooling |
| MCP runtime | Exact `rmcp` requirement in `crates/axon-mcp/Cargo.toml`, resolved in `Cargo.lock` |

Do not copy version numbers, table totals, or full command inventories into
agent instructions. Read their owning manifests or generated registries.

There are no long-lived variant branches. The former `marketplace-no-mcp`
marketplace variant, its sync/drift workflows, and its `plugins/scripts/`
helpers were retired on 2026-07-27; do not reintroduce them.

## Verification Scope Guard

- Before running tests, builds, CI reruns, or other expensive validation, classify
  the changed-file surface and pick the smallest check that proves the change.
- `.agents/skills/**`, `docs/sessions/**`, and prose-only docs changes do not
  justify full Rust, web, Android, Docker, or release CI. Use structural checks
  instead, such as file counts, required `SKILL.md` presence, symlink checks, or
  targeted formatter/parser validation for the files touched.
- Run broader tests only when code, workflow, schema, generated artifacts,
  package manifests, or runtime configuration changed, or when the user explicitly
  asks for a build/test/CI pass.
- If a workflow skill asks for baseline tests but the change is non-code, state
  the narrower verification choice and why. Do not spend minutes compiling Axon
  just to prove copied agent skills exist.

## Generated Contract Gate

Use `cargo xtask generated-contracts refresh` after changing schema inputs and
`cargo xtask generated-contracts check` to verify them. The aggregate command
always processes schema fixtures/artifacts before their Markdown projections.
For the API schema, every domain Rust provenance input is a closure root.
Production `mod`, literal `#[path]`, and literal `include!` dependencies are
followed transitively; exact `cfg(test)` items are excluded. Repository generator
code that directly shapes API bytes is tracked as explicit provenance leaves.
The central family dispatcher is not followed because its child modules own
unrelated schema families. Do not maintain a hand-written list of split domain
source modules.

## Quick Start

> **SQLite/in-process jobs are the only runtime.** Durable jobs are stored in
> the unified SQLite job tables and workers run in the same Tokio runtime. Axon
> has no Postgres, Redis, RabbitMQ, AMQP, or per-source-family queue runtime.

```bash
# Recommended: use the wrapper script (auto-sources .env)
./scripts/axon doctor
./scripts/axon https://example.com --scope site --wait true

# MCP server via CLI subcommand
./scripts/axon mcp

# Or build and run the binary directly
cargo build --release --bin axon
./target/release/axon --help

# Or build + run in one shot (does NOT auto-source .env)
cargo run --bin axon -- https://example.com --scope site --wait true
```

> **Note:** The binary is named `axon`. Build with `cargo build --bin axon`.

## MCP Server (`axon mcp`)

Axon exposes a primary `axon` tool with `action`/`subaction` routing and the
auxiliary `axon_status_dashboard` MCP App tool. Source acquisition uses
`action=source`; lifecycle operations use `jobs`; cleanup uses `prune`.
The actual catalog is defined in `crates/axon-mcp/src/server.rs`; do not
confuse the primary action schema with the complete `tools/list` catalog.
The server also implements MCP task handling in `server/tasks.rs` and its
sidecars. Treat unmerged projection-mode work as branch work, not current
main behavior.

```bash
cargo build --release --bin axon
./target/release/axon mcp
```

MCP docs:
- `docs/reference/mcp/overview.md` (runtime/design guide)
- `docs/reference/mcp/tool-schema.md` (current generated runtime snapshot)
- `docs/pipeline-unification/` records the contracts that produced the current
  clean-break runtime. Treat future-looking language inside dated delivery
  documents as historical planning context, not as the live runtime.

## Commands

The generated command registry is authoritative:
`docs/reference/cli/commands.json` (machine-readable) and
`docs/reference/cli/commands.md` (rendered). It is regenerated by `cargo xtask schemas cli`. Do not
hand-maintain a second full flag reference here.

| Group | Current commands |
|---|---|
| Unified sources | `source` (also bare `<source>`), focused projections `scrape`, `crawl`, `embed`, and `ingest`, plus `map`, `sessions`, `watch create/list/get/status/update/exec/pause/resume/delete/history` |
| Retrieval and analysis | `query`, focused `code-search`, `retrieve`, `ask`, `chat`, `summarize`, `evaluate`, `train`, `suggest`, `search`, `research`, `extract` plus its generated lifecycle conveniences, `brand`, `diff`, `endpoints`, `screenshot` |
| Durable lifecycle | `jobs list/get/events/stream/cancel/retry/recover/cleanup/clear/worker`, `status`, `monitor jobs` |
| Discovery and memory | `sources`, `domains`, `stats`, `collections list/get`, `graph kinds/resolve/query/node/edge/source`, `memory remember/list/search/show/link/supersede/context` |
| Artifacts and uploads | `artifacts list/get/content`, `uploads list/get/create/complete/abort` |
| Cleanup and storage | `prune plan`, `prune exec`, `reset plan`, `reset exec`, `migrate`, `sync pending` |
| Runtime and setup | `serve`, `serve mcp`, `mcp`, `doctor`, `doctor diagnose`, `debug`, `preflight`, `smoke`, `compose up/down/restart/rebuild`, `setup plugin-hook/init/check/targets/install/config rewrite`, `config list/get/set/unset/path`, `providers list/get`, `capabilities`, `completions`, `update`, `palette` |

Source scope replaces command-family selection:

```bash
axon https://example.com/page --scope page
axon https://example.com/docs --scope site --wait true
axon /home/user/project --wait true
axon query "provider cooling" --content-kind code
```

`scrape`, `crawl`, `embed`, and `ingest` are focused `SourceRequest` projections;
`code-search` is a committed-state query projection. They share the canonical
services and job lifecycle and do not restore separate pipelines. Use `axon jobs ...` for
the canonical durable lifecycle. The generated extract lifecycle convenience
commands (`status`, `cancel`, `errors`, `list`, `cleanup`, `clear`, `worker`,
`recover`) project over that same unified store, not a separate extract job
table. Use `axon prune ...` for cleanup.

## Architecture

The current source flow is defined by the pipeline-unification contracts and
generated runtime references linked below. Some older architecture documents
retain historical terminology and must not override those live contracts.

### Workspace layout (Rust crates)

The product is a Cargo workspace consumed by the thin root `axon` binary
(`src/main.rs` + `src/lib.rs`, which re-exports `axon_cli::run`). All crates
inherit the product version via `[workspace.package]`.

The active source pipeline is:

```text
SourceRequest
  -> resolve and route (`axon-route`)
  -> acquire (`axon-adapters`)
  -> ledger generation + manifest (`axon-ledger`)
  -> normalize/parse/prepare (`axon-document`, `axon-parse`, `axon-extract`)
  -> embed (`axon-embedding`)
  -> publish/query (`axon-vectors`, `axon-retrieval`)
  -> graph + cleanup debt (`axon-graph`, `axon-prune`)
```

Cross-cutting crates provide transport DTOs and policy (`axon-api`,
`axon-authz`, `axon-error`), configuration and shared infrastructure
(`axon-core`, `axon-llm`, `axon-observe`), durable execution (`axon-jobs`), and
composition (`axon-services`). `axon-cli`, `axon-mcp`, and `axon-web` are thin
transport adapters over those typed boundaries.

**Crate ownership rule (read before adding an operation):** own the contract where
the data lives — single-domain logic in its domain crate, the `*Result` DTO in
`axon-api`, `axon-services` as a thin facade; only cross-domain or job-runtime
work lives *in* `axon-services`. Transports never import a domain crate's
internal `::ops::*` modules. Canonical docs:
[`docs/architecture/crate-structure.md`](docs/architecture/crate-structure.md)
for the current crate map and
[`docs/architecture/crate-ownership.md`](docs/architecture/crate-ownership.md)
for the ownership decision rule; enforced by `cargo xtask check-layering`.

High-level ownership:

- `axon-api`: transport-neutral source, job, graph, memory, prune, and wire DTOs.
- `axon-adapters`: source-owned acquisition for web, local, git, feeds,
  registries, Reddit, YouTube, CLI tools, and MCP tools.
- `axon-ledger`: source identity, generations, manifests, item state, leases,
  document status, and cleanup debt.
- `axon-jobs`: one SQLite durable job model with attempts, stages, events,
  heartbeats, artifacts, reservations, recovery, and the watch scheduler.
- `axon-services`: typed orchestration and runtime composition; the source runner
  keeps one job id through acquire, prepare, embed, publish, graph, and cleanup.
- `axon-prune`: cleanup planning/execution behind the `prune` surface.
- `axon-embedding`, `axon-vectors`, `axon-retrieval`: embedding, Qdrant storage,
  hybrid retrieval, and RAG-facing vector operations.
- `axon-cli`, `axon-mcp`, `axon-web`: CLI, MCP tools/tasks/resources, and Axum REST/UI.

See `docs/pipeline-unification/foundation/source-pipeline.md`,
`docs/reference/job-lifecycle.md`, and `docs/reference/runtime/ledger.md` for
the current flow. Do not reintroduce removed crates or split source work back
into command-specific pipelines.

## Infrastructure and Deployment

The supported production Axon process is a **native binary under systemd**:
inside an Incus system container (preferred), or on bare-metal Linux. Both
run `axon serve` with the API, MCP, web panel, and in-process job workers.
See [deployment](docs/operations/deployment.md),
[Incus](deploy/incus/README.md), and [systemd](deploy/systemd/README.md).

Qdrant, TEI, and Chrome/CDP are external providers. Their endpoints may be
local containers or remote services. Do not start duplicate infrastructure
or stop an existing service merely to validate documentation.

| File | Role |
|---|---|
| `docker-compose.prod.yaml` | Reference for provider images, ports, and container topology; not the supported production Axon-process lifecycle |
| `docker-compose.yaml` | Local development overlay using the bind-mounted debug binary |
| `docker-compose.external-qdrant.yaml` | Overlay for an existing Qdrant endpoint; requires `AXON_EXTERNAL_QDRANT_URL` |
| `docker-compose.external-providers.yaml` | External embedding/reranking provider overlay |
| `docker-compose.llama.yaml` | Standalone optional llama.cpp provider |

```bash
# Provider infrastructure only; choose the mode matching your environment.
just services-up
just services-up-external-qdrant
# Health checks use the configured provider endpoints.
./scripts/axon doctor
```

Use `just --list`, the Compose files, and the deployment runbooks for exact
recipes, image versions, ports, and provider hardware requirements. Select
providers compatible with the actual execution target.

## Configuration (Two-Layer System)

Axon uses two configuration layers, both rooted under `~/.axon/`:

| Layer | File | Purpose | Secrets? |
|-------|------|---------|---------|
| Tuning knobs | `~/.axon/config.toml` | Search params, worker limits, TEI settings (also settable via env vars — env wins) | No — safe to commit |
| URLs + secrets | `~/.axon/.env` (auto-loaded) or repo `.env` | Service URLs, API keys, passwords | Yes — never commit |

**Priority:** CLI flags > env vars > `~/.axon/config.toml` > built-in defaults.

`~/.axon/` is the canonical home for axon's persistent data — `jobs.db`, `output/`, `logs/`, `artifacts/`, `screenshots/`, and `chrome-diagnostics/` all live flat under it. `AXON_DATA_DIR` defaults to `~/.axon` (no nested `axon/` subdirectory). See `docs/guides/configuration.md` for the full directory tree.

**Migration from `~/.local/share/axon`:** axon does NOT auto-migrate. Either move the directory yourself (`mv ~/.local/share/axon ~/.axon`) or set `AXON_DATA_DIR=~/.local/share/axon` to pin the old location. Tuning knobs that were previously env-only are now also accepted in `~/.axon/config.toml`.

```bash
# Set up config.toml (optional — defaults are sensible)
mkdir -m 700 ~/.axon
cp config.example.toml ~/.axon/config.toml
chmod 600 ~/.axon/config.toml

# Override config path
AXON_CONFIG_PATH=/path/to/config.toml axon ask "..."

# Malformed config.toml = hard fail with file path + line number
# Missing config.toml = silent, uses defaults
```

See `config.example.toml` at the repo root for all supported keys with defaults and docs. See `docs/guides/configuration.md` for the full environment variable reference.

## Environment Variables

`.env` is for endpoint URLs, credentials, auth/bootstrap values, and secrets.
`config.toml` is for typed tuning. Environment variables override TOML; CLI
arguments override both. The generated registries are authoritative:

- `docs/reference/config/config-toml.md`
- `docs/reference/config/env.md`
- `config.example.toml`

Minimum endpoint shape for a container runtime:

```bash
AXON_DATA_DIR=
QDRANT_URL=http://axon-qdrant:6333
TEI_URL=http://axon-tei:80
AXON_CHROME_REMOTE_URL=http://axon-chrome:6000
```

LLM synthesis is selected with `AXON_LLM_BACKEND`: `gemini-headless` (default),
`openai-compat`, or `codex-app-server`. Use the corresponding
`AXON_SYNTHESIS_*_MODEL` and backend credential/endpoint variables documented
in the generated env reference. Search uses `AXON_SEARXNG_URL` when configured,
otherwise Tavily when `TAVILY_API_KEY` is available. Git provider and Reddit
credentials remain adapter credentials even though every such target now enters
through `SourceRequest`.

Put collection, retrieval, pipeline, provider, job, watch, memory, graph,
artifact, prune, observability, and security tuning in their typed TOML
sections. Do not document removed source-family config keys as active runtime
knobs; `setup config rewrite` is the supported clean-break migration helper.

### MCP Security Env

MCP HTTP auth is selected at startup:
- `AXON_AUTH_MODE=oauth` enables the lab-auth Google OAuth/JWT flow and mounts `/.well-known/*`, `/authorize`, `/token`, `/register`, and related routes.
- `AXON_HTTP_TOKEN` enables static bearer auth and also remains accepted in OAuth dual-mode.
- OAuth email allowlisting is the access boundary. Allowed OAuth users receive full Axon server access; newly issued OAuth tokens default to both `axon:read` and `axon:write`, and either Axon scope is accepted for all Axon read/write routes for compatibility with existing tokens.
- Tokenless HTTP is allowed only for loopback development binds; non-loopback binds require either OAuth mode or a static token.

```bash
# Static bearer token accepted as Authorization: Bearer ... or x-api-key
AXON_HTTP_TOKEN=

# OAuth mode (optional; HTTP transport only)
AXON_AUTH_MODE=oauth
AXON_PUBLIC_URL=https://axon.example.com
AXON_GOOGLE_CLIENT_ID=
AXON_GOOGLE_CLIENT_SECRET=
AXON_AUTH_ADMIN_EMAIL=
AXON_ALLOWED_REDIRECT_URIS=

# MCP allowed origins (comma-separated)
AXON_ALLOWED_ORIGINS=
```

## Runtime Mode

Jobs are stored in SQLite and workers run in-process inside the same Tokio
runtime. `axon serve` and HTTP-mode `axon mcp` host the web/API/MCP surfaces and
worker runtime together. Qdrant, TEI, and Chrome/CDP are external providers;
the selected LLM backend may also be external or subprocess-backed.

```bash
axon https://example.com --scope site --wait true
```

One durable `jobs` model owns lifecycle, attempts, stages, events, heartbeats,
artifacts, and provider reservations. Source jobs keep one job id across resolve,
acquire, ledger generation, prepare, embed, publish, graph, and cleanup. There is
no child embedding handoff and no per-source-family job store.

Watches persist a canonical source request and schedule. Each due tick leases
the watch, enqueues one `source` job, and records the job id in
`axon_source_watch_runs`; the source pipeline owns the actual work. Current
watch commands are generated in `docs/reference/cli/commands.json`.

```bash
# Env vars for runtime tuning
AXON_SQLITE_PATH=/path/to/jobs.db        # optional; default: $AXON_DATA_DIR/jobs.db (i.e. ~/.axon/jobs.db)
```

See `crates/axon-jobs/src/AGENTS.md`, `crates/axon-services/src/AGENTS.md`, and
`docs/reference/job-lifecycle.md` for runtime ownership and lifecycle details.

## Gotchas

### `scrape` is a SourceRequest projection

`axon scrape <url>` is retained only for one-page clean-content output. It uses
the same web adapter, ledger, preparation, embedding, vector publication, graph,
and cleanup path as `axon <url> --scope page`; it is not an alternate pipeline.
MCP and REST callers use the canonical source action/route.

### Detached work needs workers

`source`, `extract`, `sessions`, watch execution, and job retries can return a
job id when detached. Use `--wait true` for foreground completion. A process
must be running with workers for detached jobs to advance; use the generic
`axon jobs ...` lifecycle surface to inspect or control them.

### Local code uses normal source and query paths

Index a local checkout with `axon <path>` and search it with `axon query` plus
source/path/content-kind filters. Local documents use ledger generations,
manifest diffs, document preparation, and the same vector publication path as
every other source.

### Cleanup is plan-first

Cleanup is owned by `axon-prune`. Use `axon prune plan` to produce a reviewable
plan and `axon prune exec --confirm` for destructive execution. Cleanup debt in
the source ledger records work that must be retried or reconciled.

### LLM completion backend (`AXON_LLM_BACKEND`)
All synthesis operations use the provider selected by `AXON_LLM_BACKEND`:
`gemini-headless`, `openai-compat`, or `codex-app-server`. Provider contracts
and subprocess/runtime adapters live in `crates/axon-llm/`; orchestration does
not shell out from transport handlers. For OpenAI-compatible endpoints, set the
API root rather than a full `/chat/completions` URL. Codex app-server uses an
isolated `CODEX_HOME` unless the explicit user-config opt-in is enabled.

### TEI batch size / 413 handling
The TEI provider in `crates/axon-embedding/src/tei/` splits oversized requests
after HTTP 413 responses. Configure batch and retry policy under
`[providers.embedding]`; environment overrides are documented in the generated
env reference.

### TEI retries
Embedding retries, cooldown, reservations, concurrency, and timeouts belong to
the embedding provider boundary and unified scheduler. Keep provider retry
policy out of source adapters and transport handlers.

### Text chunking
Document chunking is owned by `axon-document`. Tune markdown target/minimum and
overlap under `[pipeline.chunking]`; each published chunk becomes one vector
point.

### Collection must exist before upsert
Collection creation and vector upsert are owned by `axon-vectors`; source
adapters must not perform direct Qdrant writes.

### `migrate` — one-time collection upgrade
`axon migrate --from cortex --to cortex_v2` scrolls all points from the source,
computes BM42 sparse vectors locally from `chunk_text` payload fields, and
upserts named-mode points to the destination. After migration, set
`server.default-collection = "cortex_v2"` in `~/.axon/config.toml`.

- Source must be an **unnamed** collection (`"vectors": {"size": N}` schema); named collections are rejected with a clear error.
- Destination is created automatically if it doesn't exist; if it already exists as a named collection, migration is idempotent (re-runs upsert existing points with fresh sparse vectors).
- Progress is logged every 100 pages (~25,600 points). At 256 points/page over 2.57M points, expect 1–2 hours.
- The scroll loop uses the raw Qdrant `/points/scroll` API directly (not the shared `qdrant_scroll_pages_while` helper) to enable async upserts after each page.

Restart long-running workers after changing the default collection so their
provider caches and runtime config are refreshed.

The compose file sets `context: .`; run compose builds from the repository root.

### Subprocess stdout vs stderr
CLI commands output JSON data to stdout and progress/logs to stderr (Spinner via indicatif, tracing via `log_info`/`log_done`). Keep this split intact so server-mode and MCP callers can safely parse command output.

### Adding fields to `Config` struct
When adding a config field, update `Config::default()`, parser/TOML wiring,
`config.example.toml`, and the generated config/env registries together. Run
the schema and docs drift checks; unknown or removed clean-break keys must fail
clearly rather than being silently accepted.

## Development

### Build

```bash
cargo build --bin axon                          # debug
cargo build --release --bin axon                # release
cargo check                                     # fast type check
```

### Test

`just test` prefers `cargo nextest run --locked --workspace` and falls back to
`cargo test -q --locked` when nextest is not installed. Some suites are gated
behind the workspace `test-helpers` feature.

```bash
cargo test --workspace --features test-helpers   # what CI runs
cargo test http                                  # SSRF / URL validation tests
cargo test source                                # unified source pipeline tests
cargo test chunk_text                            # text chunking tests
cargo test -- --nocapture                        # show println! output
```

Note that `cargo check` skips `cfg(test)` modules, so it will happily pass a
misnamed sidecar `#[path]`. Use `cargo test --no-run` (or `just test`) to prove
sidecars still compile.

### Lint

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings   # what `just clippy` runs
```

### just (Recommended)

```bash
just verify      # broad integration gate; see Justfile for its current steps
just fix         # cargo fmt and clippy auto-fixes; review the resulting diff
just precommit   # broad staged gate, including secret/legacy/fetch checks
just watch-check # check, test compilation, and library tests on save
just rebuild     # check + test
just services-up # local provider infrastructure
just services-up-external-qdrant # providers with an external Qdrant
```

For prose-only changes, follow the verification scope guard above instead of
running these broad gates. `just verify` also includes operational regression
contracts and fetch-divergence checks; the Justfile is the recipe authority.

`just --list` is the authoritative recipe index; the Justfile also carries
`prod-up` / `prod-up-external-qdrant`, `container-*`, `mcp-smoke`,
`openapi-check`, `coverage-branch`, and `package-extension`.

### Run directly

```bash
# Debug binary
./target/debug/axon scrape https://example.com

# With env overrides
QDRANT_URL=http://localhost:53333 \
TEI_URL=http://myserver:52000 \
./target/release/axon query "embedding pipeline" --collection my_col
```

### Monolith Policy

Changed `.rs` files are enforced at CI and via lefthook pre-commit:
- File size: ≤ 500 lines (hard fail)
- Function size: warn at 80 lines, hard fail at 120 lines
- Exempt: `tests/**`, `benches/**`, `config/**`, `**/config.rs`
- Exceptions: add to `.monolith-allowlist`

```bash
./scripts/install-git-hooks.sh  # install lefthook once
```

### Diagnose service connectivity

```bash
axon doctor
```

Checks: Qdrant, TEI, LLM endpoint reachability.

## Database Schema

[`docs/reference/runtime/database-schema.json`](docs/reference/runtime/database-schema.json)
is the generated schema authority: use its tables, indexes, foreign keys,
migration owners, checksums, and schema version instead of a duplicated list
here. Migrations belong to their owning crates, including jobs, ledger, graph,
memory, observability, and embedding-cache storage.

Source identity/generation/document state belongs in the ledger; operation
lifecycle belongs in unified job tables. Watches enqueue ordinary source
jobs. Memory and graph stores remain separate domain concerns; provider
cache state does not belong in a source adapter.

Regenerate schema inputs before their Markdown projections using
`cargo xtask generated-contracts refresh`, then run
`cargo xtask generated-contracts check`.

## Code Style

- Rust standard style — run `cargo fmt` before committing
- `cargo clippy` clean before committing
- Errors bubble via `Box<dyn Error>` at command boundaries; internal helpers return typed errors
- Structured log output via `log_info` / `log_warn` (not `println!` in library code)
- `--json` flag enables machine-readable output on all commands that print results

### Module and test layout

Use modern Rust file-per-module layout, never `mod.rs`. Tests belong in
sibling `_tests.rs` files declared with `#[cfg(test)]` and `#[path]`.
Preserve original test-module names, cfg gates, visibility, and selectors.
`cargo check` does not compile test sidecars: use the relevant test-compile
or test target when changing them. The complete rules and examples are in
[contributing](docs/development/contributing.md#rust-module-and-test-layout).

## Claude Code Plugin Surface

The bundled plugin lives at `plugins/axon/` and ships **no binary** — install
`axon` first, then `claude plugin install <path-to-this-repo>`.

- `plugins/axon/.claude-plugin/plugin.json` — manifest. Declares `skills`,
  `commands`, `agents`, and a two-key `userConfig` (`server_url`, `api_token`).
  It must **NOT** carry a `version` key; `just validate-plugin` (part of
  `just verify`) hard-fails on one.
- `plugins/axon/.mcp.json` — HTTP transport pointed at
  `${user_config.server_url}/mcp` with the configured bearer token.
- `plugins/axon/skills/`, `commands/`, `agents/`, `references/` — the prompt
  surface.
- **No plugin hooks.** `plugins/axon/hooks/` was removed on 2026-07-27. The
  plugin no longer registers `SessionStart` or `ConfigChange` hooks, so
  `axon setup plugin-hook` and `axon memory context` are not run automatically
  on session start — invoke them explicitly, or use `/axon-deploy`.
- **`.claude/hooks/*.sh` at the repo root are unrelated.** Those are repo-level
  Claude *settings* hooks (`auto-recall.sh`, `knowledge-db.sh`,
  `memory-capture.sh`, `provision-memory.sh`, `subagent-wrapup.sh`) wired
  through `.claude/settings*.json`, not plugin hooks. Do not conflate the two.

## Worktrees

- Use `.worktrees/` under the repository root for all future git worktrees for this repo.
- Do not create new sibling worktree directories; use the repository-local location on every host. Existing sibling worktrees belong to other tasks: preserve them rather than moving or cleaning them during unrelated work.
- Before switching branches for PR or stack work, check `git worktree list` and reuse an existing `.worktrees/<branch>` checkout when present.

## Completion and publication

Inspect status and preserve existing work before staging. Use the task tracker
configured for the checkout when applicable; personal tracker setup belongs in
the local override. Run checks appropriate to the changed surface and report
any failures accurately.

Commit and publish only within the scope authorized by the user. When
publication is requested, fetch the remote, reconcile concurrent updates
without discarding changes, and use a normal non-force push. Verify remote
HEAD and local status. Do not delete unrelated worktrees, stashes, branches,
or running processes as cleanup. Never force-add ignored local instructions,
credentials, or runtime data.>

## Release Pipeline

`release/components.toml` owns release drivers, shipping paths, version
sources, and tag prefixes. The CLI uses the Axon-native bump/auto-tag flow;
managed application components use release-please. Documentation and
development-tool-only changes do not require a product version bump.

Read [the release checklist and detailed contract](docs/development/release-checklist.md)
before changing versions, publishing artifacts, or cutting a release.
