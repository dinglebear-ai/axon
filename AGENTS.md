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

The generated [CLI registry](docs/reference/cli/commands.md) and its
[JSON source](docs/reference/cli/commands.json) own commands, flags, groups,
and defaults. Regenerate with `cargo xtask schemas cli`; do not maintain
a parallel flag or command inventory here.

Sources enter through `source` or bare targets. Focused `scrape`, `crawl`,
`embed`, and `ingest` projections reuse the same SourceRequest and jobs;
`code-search` queries committed indexed state. Use `jobs` for durable
lifecycle, `watch` for recurring source requests, and plan-first `prune` or
`reset` for cleanup. Read the relevant action reference before invocation.

## Architecture

The root binary delegates to `axon-cli`. CLI, MCP, and HTTP are transport
projections over typed contracts and services, not separate implementations.
The authoritative [crate map](docs/architecture/crate-structure.md) and
[ownership rules](docs/architecture/crate-ownership.md) describe the current
workspace; `cargo xtask check-layering` enforces dependency boundaries.

```text
SourceRequest
  -> resolve/route (axon-route)
  -> acquire (axon-adapters)
  -> generation/manifest (axon-ledger)
  -> normalize/parse/prepare (axon-document, axon-parse, axon-extract)
  -> embed (axon-embedding)
  -> publish/query (axon-vectors, axon-retrieval)
  -> graph and cleanup debt (axon-graph, axon-prune)
```

Single-domain logic stays in its owning crate. Shared operation/result DTOs
belong in `axon-api`; `axon-services` is a thin facade except where it owns
cross-domain orchestration and runtime composition. Transports must not
import domain-internal `::ops::*` modules. Adapters acquire content; they do
not implement storage, embedding retries, or job queues.

`axon-ledger` owns source identity, generations, manifests, document state,
and cleanup debt. `axon-jobs` owns durable execution, watches, attempts,
stages, events, heartbeats, and provider reservations. Source work retains
one job ID throughout the pipeline. Do not restore removed crates, create
per-source pipelines, or add a child embedding handoff. Memory, graph,
authorization, observation, configuration, and provider concerns retain
their separate domain boundaries. Read the relevant scoped AGENTS.md.

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

## Configuration and Security

The [configuration guide](docs/guides/configuration.md),
[typed TOML reference](docs/reference/config/config-toml.md),
[environment registry](docs/reference/config/env.md), and `config.example.toml`
own paths, keys, defaults, and migration instructions. Priority is CLI flags,
then environment, then TOML, then built-in defaults. The canonical persistent
home is `~/.axon/`, configurable through supported path overrides.

Use TOML for typed tuning and protected environment configuration for service
endpoints, credentials, and bootstrap/auth values. Never commit real secrets
or replace existing user configuration with a template. Missing config may
use defaults; malformed or removed keys must fail clearly. Update parser,
defaults, templates, and generated registries together when adding a key.

Non-loopback HTTP requires `AXON_HTTP_TOKEN` or `AXON_AUTH_MODE=oauth`.
OAuth allowlisting and transport auth are enforced server-side; preserve
the separate panel password/session boundary. Read the
[security guide](docs/operations/security.md) before changing authorization,
origins, or public bindings. Preserve URL validation, connect-time SSRF checks,
redaction, and provider credential isolation.

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

## Runtime Invariants

Detached source, extract, session, watch, or retry work requires active
workers; a returned job ID is not completion. Use `--wait true` when foreground
completion is needed. Local code uses the normal source pipeline and
committed-state query filters, not an alternate indexing path.

Cleanup is plan-first: review `prune plan` before confirmed execution. Do not
run migrations, reset data, or restart shared providers to validate prose.
Embedding retries, cooldown, oversized-batch splitting, and concurrency belong
to the provider boundary; chunking belongs to `axon-document`; collection
creation/upsert belongs to `axon-vectors`. Keep these out of source adapters.

Synthesis uses the configured `axon-llm` backend, not shell commands in
transport handlers. Preserve isolated subprocess configuration and opt-ins.
Keep machine-readable output on stdout and progress/logs on stderr. Consult
the [operations runbook](docs/operations/operations.md) and current action
references for migration and recovery procedures instead of copying changing
operational recipes into this guide.

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
just services-up # start self-contained local infra (Qdrant + TEI + Chrome)
just services-up-external-qdrant # start TEI + Chrome with external Qdrant
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
credentials, or runtime data.

## Release Pipeline

`release/components.toml` owns release drivers, shipping paths, version
sources, and tag prefixes. The CLI uses the Axon-native bump/auto-tag flow;
managed application components use release-please. Documentation and
development-tool-only changes do not require a product version bump.

Read [the release checklist and detailed contract](docs/development/release-checklist.md)
before changing versions, publishing artifacts, or cutting a release.
