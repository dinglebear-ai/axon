# Axon CLI
Last Modified: 2026-07-16

Unified source acquisition, document preparation, indexing, retrieval, and RAG
in one Rust binary backed by SQLite, Qdrant, TEI, Chrome/CDP, and a configured
LLM provider.

## Long-Lived Branches

- `marketplace-no-mcp` is an intentional long-lived marketplace variant branch,
  not stale cleanup. It keeps Axon's plugin/skill surface available while
  removing bundled MCP server registration for environments where Axon is
  already connected through the Labby gateway.
- Do not merge `marketplace-no-mcp` into `main` by default, and do not delete it
  as stale unless Jacob explicitly retires the no-MCP marketplace variant.

## Verification Scope Guard

- Before running tests, builds, CI reruns, or other expensive validation, classify
  the changed-file surface and pick the smallest check that proves the change.
- `.agents/skills/**`, `docs/sessions/**`, and prose-only docs changes do not
  justify full Rust, web, Android, Docker, or release CI. Use structural checks
  instead, such as file counts, required `SKILL.md` presence, symlink checks, or
  targeted formatter/parser validation for the files touched.
- Run broader tests only when code, workflow, schema, generated artifacts,
  package manifests, or runtime configuration changed, or when Jacob explicitly
  asks for a build/test/CI pass.
- If a workflow skill asks for baseline tests but the change is non-code, state
  the narrower verification choice and why. Do not spend minutes compiling Axon
  just to prove copied agent skills exist.

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

Axon ships an MCP server subcommand that exposes one `axon` tool with
`action`/`subaction` routing. Source acquisition uses `action=source`; lifecycle
operations use `jobs`; cleanup uses `prune`.

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
`docs/reference/cli/commands.md` (rendered). Do not hand-maintain a second full
flag reference here.

| Group | Current commands |
|---|---|
| Unified sources | `source` (also bare `<source>`), retained one-page projection `scrape`, `map`, `sessions`, `watch create/list/get/status/update/exec/pause/resume/delete/history` |
| Retrieval and analysis | `query`, `retrieve`, `ask`, `summarize`, `evaluate`, `train`, `suggest`, `search`, `research`, `extract` plus its generated lifecycle conveniences, `brand`, `diff`, `endpoints`, `screenshot` |
| Durable lifecycle | `jobs list/get/events/stream/cancel/retry/recover/cleanup/clear`, `status`, `monitor jobs` |
| Discovery and memory | `sources`, `domains`, `stats`, `memory remember/list/search/show/link/supersede/context` |
| Cleanup and storage | `prune plan`, `prune exec`, `reset`, `migrate`, `sync pending` |
| Runtime and setup | `serve`, `serve mcp`, `mcp`, `doctor`, `doctor diagnose`, `debug`, `preflight`, `smoke`, `compose up/down/restart/rebuild`, `setup`, `config`, `completions`, `update`, `palette` |

Source scope replaces command-family selection:

```bash
axon https://example.com/page --scope page
axon https://example.com/docs --scope site --wait true
axon /home/user/project --wait true
axon query "provider cooling" --content-kind code
```

`scrape` is intentionally retained as a one-page `SourceRequest` projection.
The former source-family commands have no parser variants, hidden aliases,
separate job lifecycle, or compatibility dispatchers. Use `axon jobs ...` for
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
internal `::ops::*` modules. Canonical doc:
[`docs/architecture/crate-ownership.md`](docs/architecture/crate-ownership.md);
enforced by `cargo xtask check-layering`.

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
- `axon-cli`, `axon-mcp`, `axon-web`: CLI, the single MCP tool, and Axum REST/UI.

See `docs/pipeline-unification/foundation/source-pipeline.md`,
`docs/reference/job-lifecycle.md`, and `docs/reference/runtime/ledger.md` for
the current flow. Do not reintroduce removed crates or split source work back
into command-specific pipelines.

## Infrastructure

### Docker Compose

The production stack and local development stack are split:

| File | Contents | Env file |
|------|----------|----------|
| `docker-compose.prod.yaml` | Axon server, Qdrant, Chrome, TEI (qdrant is **mandatory** here — every `up -d` starts all 4) | `~/.axon/.env` |
| `docker-compose.yaml` | Local dev stack; extends production services and runs `axon` from the bind-mounted local debug binary in `target/debug` | `~/.axon/.env` |
| `docker-compose.external-qdrant.yaml` | Overlay (apply **on top of** `-f docker-compose.prod.yaml`) that drops the bundled `axon-qdrant` and points axon at an existing external Qdrant. Requires `AXON_EXTERNAL_QDRANT_URL` (fails loudly if unset). **This homelab's mode — Qdrant lives on tootie, off dookie for RAM reasons.** Recipes: `just prod-up-external-qdrant` / `just prod-down-external-qdrant`. | `~/.axon/.env` |
| `docker-compose.llama.yaml` | Standalone llama.cpp GGUF server (own `llama-cpp` project, joins the external `${DOCKER_NETWORK:-axon}` network) for a local OpenAI-compatible endpoint at `:${LLAMA_CPP_PORT:-8080}/v1`. Not part of the axon stack. | — |

**GPU acceleration:** On NVIDIA hosts, `docker-compose.prod.yaml` includes NVIDIA reservations for `axon-tei`.

```bash
docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml up -d
```

For local dev:

```bash
cargo build --bin axon
docker compose --env-file ~/.axon/.env -f docker-compose.yaml up -d axon
```

CPU-only hosts should override the TEI image/settings or run an external TEI endpoint.

### Infrastructure Services (`docker-compose.prod.yaml`)

| Service | Image | Exposed Port | Purpose |
|---------|-------|-------------|---------|
| `axon-qdrant` | qdrant/qdrant:v1.18.2 | `53333`, `53334` (gRPC) | Vector store |
| `axon-tei` | ghcr.io/huggingface/text-embeddings-inference:89-1.9 | `52000` | Embedding generation (GPU, NVIDIA) |
| `axon-chrome` | built from config/chrome/Dockerfile | `6000` (management), `9222` (CDP proxy) | headless_browser + chrome-headless-shell |

```bash
# Start infrastructure (qdrant, tei, chrome)
just services-up
# or: docker compose --env-file ~/.axon/.env -f docker-compose.yaml up -d axon-qdrant axon-tei axon-chrome

# Check infra health
docker compose --env-file ~/.axon/.env ps

# Stop everything
just services-down
```

## Configuration (Two-Layer System)

Axon uses two configuration layers, both rooted under `~/.axon/`:

| Layer | File | Purpose | Secrets? |
|-------|------|---------|---------|
| Tuning knobs | `~/.axon/config.toml` | Search params, worker limits, TEI settings (also settable via env vars — env wins) | No — safe to commit |
| URLs + secrets | `~/.axon/.env` (auto-loaded) or repo `.env` | Service URLs, API keys, passwords | Yes — never commit |

**Priority:** CLI flags > env vars > `~/.axon/config.toml` > built-in defaults.

`~/.axon/` is the canonical home for axon's persistent data — `jobs.db`, `output/`, `logs/`, `artifacts/`, `screenshots/`, and `chrome-diagnostics/` all live flat under it. `AXON_DATA_DIR` defaults to `~/.axon` (no nested `axon/` subdirectory). See `docs/guides/configuration.md` for the full directory tree.

**Migration from `~/.local/share/axon`:** axon does NOT auto-migrate. Either move the directory yourself (`mv ~/.local/share/axon ~/.axon`) or set `AXON_DATA_DIR=~/.local/share` to pin the old location. Tuning knobs that were previously env-only are now also accepted in `~/.axon/config.toml`.

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

See `crates/axon-jobs/src/CLAUDE.md`, `crates/axon-services/src/CLAUDE.md`, and
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

```bash
cargo test                    # run all tests
cargo test http               # SSRF / URL validation tests (21)
cargo test source             # unified source pipeline tests
cargo test chunk_text         # text chunking tests (7)
cargo test -- --nocapture     # show println! output
```

### Lint

```bash
cargo clippy
cargo fmt --check
```

### just (Recommended)

```bash
just verify      # fmt-check + clippy + check + test (pre-PR gate)
just fix         # cargo fmt + clippy --fix (auto-repair)
just precommit   # full pre-commit: monolith check + verify
just watch-check # cargo watch: check + test-lib on every file save
just rebuild     # check + test
just services-up # start infra (qdrant, tei, chrome)
just services-down # stop infra
just stop        # stop running mcp and worker processes
```

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

`docs/reference/runtime/database-schema.json` is the authoritative generated
schema. Its current clean-break epoch is 1 and it contains 29 domain tables
parsed from 7 canonical migration files. Keep this summary grouped by ownership; use the JSON for
columns, indexes, foreign keys, and migration provenance.

| Family | Current tables |
|---|---|
| Unified jobs | `jobs`, `job_attempts`, `job_stages`, `job_events`, `job_heartbeats`, `job_artifacts`, `provider_reservations`, `config_snapshots` |
| Source ledger | `sources`, `source_generations`, `source_manifests`, `source_items`, `document_status`, `cleanup_debt`, `leases` |
| Source watches | `axon_source_watches`, `axon_source_watch_runs` |
| Observability | `axon_observe_events`, `axon_observe_heartbeats`, `axon_observe_provider_health` |
| Source graph | `graph_nodes`, `graph_aliases`, `graph_edges`, `graph_evidence`, `graph_conflicts` |
| Memory | `axon_memory_nodes`, `axon_memory_edges`, `memory_records`, `memory_links`, `memory_reinforcement`, `memory_reviews` |

Migrations run through the owning stores in `axon-jobs`, `axon-ledger`,
`axon-graph`, and `axon-memory`. Source lifecycle state belongs in the ledger;
operation lifecycle belongs in the unified job tables.

## Code Style

- Rust standard style — run `cargo fmt` before committing
- `cargo clippy` clean before committing
- Errors bubble via `Box<dyn Error>` at command boundaries; internal helpers return typed errors
- Structured log output via `log_info` / `log_warn` (not `println!` in library code)
- `--json` flag enables machine-readable output on all commands that print results

### Module Layout — Modern Rust Convention (ENFORCED)

**Never use `mod.rs`.** Use the Rust 2018+ file-per-module layout:

```plaintext
# WRONG — do not do this
foo/
└── mod.rs      ← forbidden

# CORRECT
foo.rs          ← module root lives here
foo/
├── bar.rs      ← submodule
└── baz.rs      ← submodule
```

- Module root always lives in `foo.rs`, never `foo/mod.rs`
- Submodules live in `foo/bar.rs`, declared with `mod bar;` inside `foo.rs`
- When splitting an existing `foo/mod.rs`: copy it to `foo.rs`, delete `foo/mod.rs` — the submodule files stay in `foo/` unchanged
- This applies everywhere: `src/`, `src/*/`, nested modules — no exceptions

### Test files — sidecar `_tests.rs` convention (ENFORCED)

**Tests live in sibling files**, not inline `#[cfg(test)] mod tests { ... }` blocks. For each source file with tests, create a sibling `_tests.rs` file and declare it inside the source with the `#[path]` attribute:

```plaintext
foo.rs          ← source code
foo_tests.rs    ← sidecar test file (one per original `#[cfg(test)] mod X` block)
```

In `foo.rs`:

```rust
#[cfg(test)]
#[path = "foo_tests.rs"]
mod tests;
```

In `foo_tests.rs`:

```rust
use super::*;  // tests still see foo.rs's private items

#[test]
fn it_works() { ... }
```

**Rules:**

- **One sidecar per original `#[cfg(test)] mod X { ... }` block.** Never wrap multiple blocks under a single `mod tests` — this breaks `cargo test foo::<orig_mod_name>::test_x` selectors and risks visibility escalation. If `foo.rs` had `mod tests`, `mod legacy`, and `mod proptest_tests`, emit three sidecar files: `foo_tests.rs`, `foo_legacy_tests.rs`, `foo_proptest_tests.rs`, with three matching `#[path]` declarations in `foo.rs`.
- **Source-side `mod` name must match the original block's mod name** (`mod legacy`, `mod proptest_tests`, not always `mod tests`). Test selectors stay identical to pre-migration.
- **Why `#[path]`?** It decouples disk location from module hierarchy. The file is a sibling of `foo.rs` on disk, but the module is a **child** of `foo`, so `use super::*;` keeps private-item access. A sibling-declared `mod foo_tests;` (without `#[path]`) would make `foo_tests` a sibling of `foo` in the module tree and lose private access.
- **Compound cfg gates carry over.** A source with `#[cfg(all(test, unix))]` becomes:

  ```rust
  #[cfg(all(test, unix))]
  #[path = "foo_tests.rs"]
  mod tests;
  ```

  The sidecar inherits the parent's gate; do not re-gate items inside it.

- **`mod test_support;` and other non-`#[cfg(test)]` helper modules are NOT sidecars** — they stay declared as regular submodules with their files in `foo/`.
- **Footgun.** If a sidecar `foo_tests.rs` itself declares `mod bar;` (without `#[path]`), rustc resolves `bar` relative to the sidecar's on-disk location and looks for `foo_tests/bar.rs`, *not* `foo/bar.rs`. Inline the submodule or pass an explicit `#[path]` from the sidecar.
- **Monolith policy.** `**/*_tests.*` is exempt from the 500-line cap — sidecars can hold large test suites without splitting.
- **No `xtask` CI guardrail** for inline-test regressions; the convention is enforced by docs + reviewer attention. The pre-commit `test` hook runs `cargo test --no-run --workspace --lib --locked` which compiles every sidecar — broken `#[path]` strings fail there. Do not rely on `cargo check` alone; it skips `cfg(test)` modules and will pass a misnamed `#[path]`.
- **`#[cfg(test)] impl` blocks stay in the source file.** Inherent impls of a parent type can't move to a sibling module (orphan rules apply to traits, but inherent impls must live with the type). If you have `#[cfg(test)] impl Foo { fn test_only_helper() {} }` in the source, leave it inline.
- **Block-scoped `use` semantics shift.** Inside an inline `mod tests { ... }`, `use super::X;` and similar imports are scoped to the block. After moving to a sidecar, those imports become file-scoped (still inside the same module, but visible to every test in the file). Always use `use super::*;` in sidecars — it keeps private-item access and matches the sidecar convention.
- **Directory-split footgun.** If `foo.rs` later splits into `foo/sub.rs`-style submodules (and the source moves into `foo/`), the `#[path = "foo_tests.rs"]` string is now relative to the new source's directory, not the old one. Move the `_tests.rs` files to match, or update the `#[path]` to the correct relative location. Mitigated by the test-compile gate above, but watch for it during structural refactors.

Worked examples are common under `crates/*/src/`: pair `foo.rs` with
`foo_tests.rs`, and use additional named sidecars when one source module has
multiple test modules.

## Worktrees

- Use `.worktrees/` under the repository root for all future git worktrees for this repo.
- Do not create sibling worktrees under `/home/jmagar/workspace/` for new Axon work.
- Before switching branches for PR or stack work, check `git worktree list` and reuse an existing `.worktrees/<branch>` checkout when present.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->


## Release Pipeline

### How releases work

Releases are **per-component and selective**. **Three of four components**
(`palette`, `android`, `chrome`) are release-please-managed: release PRs are
generated by release-please after `CI` succeeds on `main`, and release-please
owns version edits, changelog edits, tags, and GitHub Release records for
them. Axon's artifact workflows attach binaries/APKs/zips to the
release-please-created release by tag.

**The `cli` component is NOT release-please-managed.** release-please's Cargo
support crashes building any candidate PR that touches this repo's root
workspace, because it can't handle `version.workspace = true` (the standard
Cargo workspace-inheritance pattern every crate here uses) — this is a
confirmed, still-open upstream bug: [googleapis/release-please#2478](https://github.com/googleapis/release-please/issues/2478).
The candidate-PR build for `.` failed identically across every
release-please-action major version tested (v4 and v5, i.e. release-please
core 17.6.0 and the current 17.10.2 as of 2026-07); no config flag
(`release-type`, `extra-files`, `always-link-local`) works around it. `.` was
therefore removed from `release-please-config.json` and
`.release-please-manifest.json`. **`cli` is bumped manually with `cargo xtask
bump-version cli patch|minor|major`** — see "Version bumping rules" below.
`release/components.toml`'s `cli` entry sets `release_please_managed = false`
to document this and exempt it from the manifest-consistency check that the
other three components still get.

`release/components.toml` remains Axon's local validation and artifact dispatch
source of truth. It defines component shipping paths, tag prefixes, release
workflows, version sources, version-bearing files, release-please package
paths, and (for `cli` only) `release_please_managed = false`:

| Component | Shipping paths | Version source | Tag prefix | Release workflow | release-please managed |
|-----------|----------------|----------------|-----------|------------------|------------------------|
| **cli** (Linux + Windows; web panel bundled in) | `src`, `Cargo.toml`/`Cargo.lock`, `build.rs`, `migrations`, `apps/web`, `rust-toolchain.toml`, `vendor` | `Cargo.toml` `[package]` version | `v` | `release.yml` | **No — manual bump** |
| **palette** (Linux + Windows) | `apps/palette-tauri` | `apps/palette-tauri/src-tauri/tauri.conf.json` | `palette-v` | `palette-release.yml` | Yes |
| **android** (APK) | `apps/android` | `apps/android/app/build.gradle.kts` `versionName` | `android-v` | `android-release.yml` | Yes |
| **chrome** (extension zip) | `apps/chrome-extension` | `apps/chrome-extension/manifest.json` `version` | `chrome-ext-v` | `chrome-extension-release.yml` | Yes |

For each component, the shared release checker diffs that component's shipping
paths against its most recent tag and verifies the component version source
and all version-bearing files agree (plus, for release-please-managed
components, that the release-please manifest agrees too). After
release-please creates a release for `palette`/`android`/`chrome`,
`.github/workflows/release-please.yml` dispatches that component's artifact
workflow. `cli`'s `release.yml` still triggers the normal way, off its
`vX.Y.Z` tag push — nothing about that trigger changed, only what puts the tag
there (manual now, not a merged release-please PR).

**Implications:**

- A change touching only one component releases only that component — e.g. an
  `apps/android/**`-only change cuts an Android release and nothing else; it
  does **not** rebuild the CLI.
- Dev-only trees (`xtask`, `benches`, `.github`, `docs`, and non-shipping
  repo policy/config files) are
  **not** in any component's shipping paths, so a tooling/docs-only merge cuts
  no release and needs no version bump. `Cargo.toml`, `Cargo.lock`, and
  `rust-toolchain.toml` are CLI shipping paths and are not part of this carve-out.
- If a component's code changed but its version was **not** bumped (the tag
  already exists), `cargo xtask check-release-versions --base origin/main --head
  HEAD --mode pr` fails before merge with a message naming the component —
  this still applies to `cli`, it just means "someone forgot to run `cargo
  xtask bump-version cli`" instead of "release-please hasn't opened its PR
  yet".

To cut a release manually (e.g. re-release or hotfix without a code change),
push the component's tag directly:

```bash
git tag vX.Y.Z         && git push origin vX.Y.Z          # cli
git tag palette-vX.Y.Z && git push origin palette-vX.Y.Z  # palette
git tag android-vX.Y.Z && git push origin android-vX.Y.Z  # android
git tag chrome-ext-vX.Y.Z && git push origin chrome-ext-vX.Y.Z  # chrome
```

### Version bumping rules

Release-please is the release PR, version bump, changelog, tag, and GitHub
Release path for `palette`/`android`/`chrome`. Do not run or reintroduce
git-cliff-backed release bumping. `cli` is the one exception — bump it with
`cargo xtask bump-version cli patch|minor|major`, choosing the level yourself
(no commit-message parsing, since there's no release-please PR to compute it
from):

```bash
cargo xtask bump-version cli patch   # or minor / major
```

This writes every one of `cli`'s version-bearing files below in one shot
(including running `cargo update -p axon --precise X.Y.Z` to refresh
`Cargo.lock`, and every workspace member's own `Cargo.lock` entry that
inherits via `version.workspace = true`), inserts a dated `## [X.Y.Z]`
`CHANGELOG.md` heading, and is idempotent — a second run at the same version
is a no-op. It is a pure text-substitution writer (deliberately not a
`serde_json`/full-manifest round-trip — that reformatted
`apps/web/openapi/axon.json` wholesale in testing), so it only ever touches
the specific version field, byte-for-byte, everywhere else in the file
untouched.

Release-please (for `palette`/`android`/`chrome`) determines the bump type
from conventional commits:

- `feat!:` or `BREAKING CHANGE` → **major** (X+1.0.0)
- `feat` or `feat(...)` → **minor** (X.Y+1.0)
- `fix` or `fix(...)` → **patch** (X.Y.Z+1)
- `perf` and `refactor` appear in the **Changed** changelog section when they
  are part of a release.
- `chore`, `ci`, `docs`, `test`, `build`, and `style` are hidden from
  generated release notes by `release-please-config.json` so non-user-facing
  maintenance commits do not bury the release signal.

For `cli`, pick the equivalent level yourself using the same rules when
choosing `patch`/`minor`/`major`.

**CLI component — all of these MUST move together (Cargo.toml is the source of truth; `cargo xtask bump-version cli` handles all of it):**
- `Cargo.toml` — `version = "X.Y.Z"` in both `[package]` and `[workspace.package]` (Cargo.lock follows automatically)
- `README.md` — version header (no longer carries the `x-release-please-version` marker — release-please doesn't touch this file anymore)
- `CHANGELOG.md` — new entry under the bumped version
- `apps/web/package.json` + `apps/web/openapi/axon.json` — `"version": "X.Y.Z"`

**Palette component — all three MUST move together (release-please-managed):**
- `apps/palette-tauri/src-tauri/tauri.conf.json`, `apps/palette-tauri/package.json`,
  `apps/palette-tauri/src-tauri/Cargo.toml`

**Android component (release-please-managed):** `apps/android/app/build.gradle.kts` `versionName` (bump
`versionCode` too). **Chrome component (release-please-managed):** `apps/chrome-extension/manifest.json`.

`plugins/axon/.claude-plugin/plugin.json` must **NOT** carry a `version` key —
`just validate-plugin` (part of `just verify`) hard-fails on it; the plugin is
versioned by the marketplace, not the manifest.

**Changelogs are generated, not hand-stamped.** Each component has its own
`CHANGELOG.md` (`CHANGELOG.md`, `apps/palette-tauri/CHANGELOG.md`,
`apps/android/CHANGELOG.md`, `apps/chrome-extension/CHANGELOG.md`).
For `palette`/`android`/`chrome`, release-please owns changelog updates and
uses the native `changelog-sections` policy in `release-please-config.json`
to mirror Axon's old release-note shape: user-facing `feat`, `fix`, `perf`,
and `refactor` entries are shown; routine maintenance types are hidden.
Release PRs also carry release-please labels and a PR header/footer
explaining that CI may append derived-file fixups before merge. For `cli`,
`cargo xtask bump-version cli` inserts the dated heading itself (no
generated commit-message-derived body — write the entry by hand if you want
one beyond the heading).

`xtask` is a validation, manual-bump, and release-please postprocessing
helper: `check-release-versions` verifies component parity and changed
shipping paths (skipping the release-please-manifest check for `cli`, since
it has no manifest entry to check against), `bump-version` is the manual
writer for `cli`, `release-please-fixup-plan`/`release-please-fixups` handle
derived files release-please cannot update directly for
`palette`/`android`/`chrome`, and `release-please-dispatch-plan` translates
release-please outputs into artifact workflow dispatches. **Editing a
`CHANGELOG.md` never triggers a release** — change detection ignores it, so
documenting a release can't recursively cut another.

The PR gate is:

```bash
cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
```

Short release checklist:

- **`palette`/`android`/`chrome`:**
  1. Let release-please open the release PR after green `CI` on `main`.
  2. Review that the release PR updates `.release-please-manifest.json`, component version, and changelog.
  3. Run `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`.
  4. Merge only after the release/version gate and CI are green.
- **`cli`:**
  1. Run `cargo xtask bump-version cli patch|minor|major` locally, picking the level yourself.
  2. Review the diff — every file listed above should move together, nothing else.
  3. Include the bump in your PR (or a dedicated bump PR) and run `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`.
  4. Merge; `.github/workflows/auto-tag.yml` — unaffected by any of this, since it's driven entirely by `release/components.toml`, not release-please's manifest — detects the bumped-but-untagged `cli` version, creates and pushes the `vX.Y.Z` tag once `CI` is green, and dispatches `release.yml` exactly as it always has.

The compatibility command `cargo xtask check-version-sync` still enforces
**CLI** version parity across `Cargo.toml`, `README.md`, `CHANGELOG.md`,
`apps/web/package.json`, and `apps/web/openapi/axon.json`, and checks that
`plugins/axon/.claude-plugin/plugin.json` has no `version` key. The full
multi-component gate is `cargo xtask check-release-versions`.

<!-- OPENWIKI:START -->

## OpenWiki

This repository uses OpenWiki for recurring code documentation. Start with `openwiki/quickstart.md`, then follow its links to architecture, workflows, domain concepts, operations, integrations, testing guidance, and source maps.

The scheduled OpenWiki GitHub Actions workflow refreshes the repository wiki. Do not hand-edit generated OpenWiki pages unless explicitly asked; prefer updating source code/docs and letting OpenWiki regenerate.

<!-- OPENWIKI:END -->
