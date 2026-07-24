# Axon

Version: 7.1.5

Axon is a self-hosted RAG stack: source acquisition, document preparation,
embedding, hybrid retrieval, and LLM synthesis in one Rust binary, backed by
SQLite, Qdrant, Hugging Face TEI, and Chrome/CDP. Every source — a web page,
a site, a local checkout, a Git repo, a package, a Reddit subreddit, a YouTube
transcript, an AI session export — enters through one unified pipeline.

Axon runs as a **native binary under systemd**: an Incus system container is the
preferred deployment, bare-metal systemd is a supported alternative. The
infrastructure it depends on (Qdrant, TEI, Chrome) typically runs as containers
or on external hosts; Axon reaches them by URL.

## The unified source pipeline

All source acquisition, refresh, watch, indexing, graph extraction, embedding,
publishing, and cleanup flow through one pipeline. CLI, MCP, and REST are thin
transport projections over the same `SourceRequest` DTO.

```text
SourceRequest
  → resolve and route (`axon-route`)
  → acquire (`axon-adapters`)
  → ledger generation + manifest (`axon-ledger`)
  → normalize / parse / prepare (`axon-document`, `axon-parse`, `axon-extract`)
  → embed (`axon-embedding`)
  → publish / query (`axon-vectors`, `axon-retrieval`)
  → graph + cleanup debt (`axon-graph`, `axon-prune`)
```

One durable `job_id` crosses every stage — logs, events, ledger rows, graph
updates, artifacts, vector payloads, and status all share it. There is no
per-source-family pipeline and no per-family job store.

The design contract packet that produced this runtime lives in
[`docs/pipeline-unification/`](docs/pipeline-unification/README.md); treat it as
the historical design record, not as future work — the clean break is
implemented.

## Deployment contract

**Supported ways to run the axon binary:**

- **Incus system container (preferred).** `deploy/incus/bootstrap.sh` brings up
  one Incus system container that runs axon native under a systemd unit
  (`axon-native.service`) and Qdrant/TEI/Chrome as nested containers, with GPU
  passthrough. See [`deploy/incus/README.md`](deploy/incus/README.md) for the
  profile, storage, and GPU details.
- **Bare-metal systemd (supported).** Install the binary, drop in
  [`deploy/systemd/axon.service`](deploy/systemd/axon.service), enable it. See
  [`deploy/systemd/README.md`](deploy/systemd/README.md) for the walkthrough.

Both paths run `/usr/local/bin/axon serve`, which hosts the HTTP API (`/v1/*`),
MCP-over-HTTP (`/mcp`), the web control panel, and the in-process worker runtime
in one process on `127.0.0.1:8001` by default.

**Not supported as an axon deployment path:** running the axon binary itself in
a Docker container as the production deployment. (Container images are still
published to GHCR for users who want them, and `docker-compose.prod.yaml`
remains the canonical reference for infra image versions and ports — but the
supported production deployments are the two above.)

**Not supported at all:** Postgres, Redis, RabbitMQ, AMQP, external worker
services, Neo4j graph retrieval, or multiple competing `.env`/`config.toml`
locations. Jobs are stored in SQLite and workers run in the same Tokio runtime
as `axon serve`.

**Target hardware:** local NVIDIA RTX 4070 with NVIDIA Container Toolkit (for
TEI GPU throughput). Axon itself is CPU-only.

## Install the binary

The installers fetch a release binary and delegate the rest to `axon setup`.
Deployment (Incus or systemd) is a separate step above.

### Linux

Prerequisites: Linux x86_64, `curl`, `sha256sum`, `install`, and (for GPU
synthesis or a configured OpenAI-compatible endpoint) the relevant credentials.

One-line installer:

```bash
curl -fsSL https://raw.githubusercontent.com/jmagar/axon/main/install.sh | sh
```

The installer verifies the release checksum and installs `axon` to
`~/.local/bin/axon`. Useful controls:

```bash
AXON_INSTALL_DRY_RUN=1 ./install.sh
AXON_INSTALL_PREFIX=/opt/axon ./install.sh
AXON_VERSION=vX.Y.Z ./install.sh          # pin a release; defaults to latest
AXON_INSTALL_SKIP_SETUP=1 ./install.sh    # skip the axon setup handoff
AXON_INSTALL_METHOD=build ./install.sh    # cargo build --release instead of pulling
```

### Windows

Prerequisites: Windows x86_64 (PowerShell 5.1+ or PowerShell Core).

```powershell
irm https://raw.githubusercontent.com/jmagar/axon/main/install.ps1 | iex
```

Installs `axon.exe` to `%USERPROFILE%\.local\bin` and adds it to the user PATH.
Controls: `AXON_INSTALL_DRY_RUN`, `AXON_INSTALL_PREFIX`, `AXON_VERSION`,
`AXON_INSTALL_SKIP_SETUP`.

### Claude Code plugin

```bash
claude plugin install <path-to-this-repo>
```

The plugin ships no binary — install `axon` first. Its `SessionStart` hook runs
`scripts/plugin-setup.sh`, which syncs `CLAUDE_PLUGIN_OPTION_*` settings into
process env and delegates to `axon setup plugin-hook`. That subcommand is
**probe-only and never deploys**: it checks `/readyz` and exits silently when the
stack is up, or prints a one-line `run /axon-deploy` advisory when it is down.
Provisioning is the `/axon-deploy` slash command (or `axon setup`).

## Deploy

### Incus (preferred)

`deploy/incus/bootstrap.sh` is an idempotent 19-step bootstrap that: launches an
`images:debian/bookworm` system container under `axon-container-profile`, applies
GPU passthrough, builds axon from the repo's Dockerfile `runtime` stage, installs
`axon-native.service`, brings up Qdrant/TEI/Chrome as nested containers, and
optionally exposes axon's port to the host via an Incus `proxy` device. A host
`axon-incus-bootstrap.service` unit re-runs it on host boot.

```bash
incus profile create axon-container-profile
incus profile edit axon-container-profile < deploy/incus/profile.yaml
AXON_EXTERNAL_QDRANT_URL=http://qdrant-host:53333 \
AXON_ENV_FILE=/home/you/.axon/.env \
./deploy/incus/bootstrap.sh
```

See [`deploy/incus/README.md`](deploy/incus/README.md) for the split-model
rationale (axon native, infra nested), storage fork (`~/.axon-incus`), and the
`nvidia-procfs` fragility note.

### Bare-metal systemd

```bash
useradd --system --home /var/lib/axon --shell /usr/sbin/nologin axon
install -d -o axon -g axon /var/lib/axon
install -m 0755 axon /usr/local/bin/axon
install -d /etc/axon && install -m 0600 your.env /etc/axon/axon.env
install -m 0644 deploy/systemd/axon.service /etc/systemd/system/axon.service
systemctl daemon-reload && systemctl enable --now axon
```

See [`deploy/systemd/README.md`](deploy/systemd/README.md) for the full
walkthrough, hardening directives, and infra notes.

## Configuration home and setup

`~/.axon/` is the canonical home for persistent data on bare-metal (under Incus
it's `/mnt/axon-data`, mapped from `~/.axon-incus` on the host). It is flat — no
nested `axon/` subdirectory:

```text
~/.axon/
├── .env              URLs, secrets, auth, runtime bootstrap
├── config.toml       non-secret tuning (search, workers, chunking, providers)
├── jobs.db           SQLite: unified jobs + source ledger + graph + memory
├── output/           prepared docs, vectors, manifests
├── logs/axon.log     rotated logs
├── artifacts/        large/raw source outputs
├── screenshots/      captured pages
└── chrome-diagnostics/
```

`AXON_DATA_DIR` defaults to `~/.axon`. If you previously used
`~/.local/share/axon`, axon does not auto-migrate — either `mv` it to
`~/.axon` or set `AXON_DATA_DIR=~/.local/share` to pin the old location.

`axon setup init` creates `~/.axon`, `config.toml`, and `.env` idempotently,
filling only missing runtime values and preserving secrets. Focused commands:

```bash
axon setup init        # create ~/.axon, config.toml, .env
axon preflight         # check prerequisites, auth, service readiness
axon doctor            # check Qdrant, TEI, LLM endpoint reachability
axon smoke             # TEI prewarm + source/ask proof
axon setup plugin-hook # probe-only SessionStart path (never deploys)
```

### Two-layer config

| File | Purpose |
|---|---|
| `~/.axon/.env` | URLs, secrets, auth, runtime bootstrap values |
| `~/.axon/config.toml` | non-secret tuning and behavior |

Precedence: **CLI flags > env vars > `~/.axon/config.toml` > built-in defaults.**

`.env` keeps endpoint URLs (`QDRANT_URL`, `TEI_URL`, `AXON_CHROME_REMOTE_URL`,
`AXON_SEARXNG_URL`), secrets (`AXON_HTTP_TOKEN`, `TAVILY_API_KEY`,
`GITHUB_TOKEN`, `GITLAB_TOKEN`, `GITEA_TOKEN`, Reddit credentials, `HF_TOKEN`,
OAuth credentials), Docker/runtime bootstrap, and LLM runtime pointers.
`config.toml` keeps search/ask tuning, worker and job limits, TEI client
tuning, Qdrant batch sizing, chunking, and logging. See
[`config.example.toml`](config.example.toml), [`.env.example`](.env.example),
and [`docs/guides/configuration.md`](docs/guides/configuration.md) for the full
surface.

## Quick start

For the full first-run walkthrough see
[`docs/guides/quickstart.md`](docs/guides/quickstart.md) and
[`docs/guides/getting-started.md`](docs/guides/getting-started.md). After deploy
+ setup:

```bash
axon doctor
axon https://example.com --scope site --wait true
axon query "provider cooling" --content-kind code
axon ask "What did we index?"
```

CLI and `axon mcp` (stdio) run actions in-process against Qdrant and TEI.
`axon serve` exposes the same operations over HTTP for deployed instances.

## Sources and scopes

One command acquires any source; scope selects the acquisition strategy:

```bash
axon https://example.com/page          --scope page      # one page (default)
axon https://example.com/docs          --scope site      # crawl a site
axon /home/you/project                                    # local checkout
axon github.com/owner/repo             --scope repo      # hosted git
axon pypi.org/project/django                             # package registry
axon reddit.com/r/rust                 --scope subreddit # Reddit
```

Adapter families: `web`, `local`, `github`, `gitlab`, `gitea`/`git`, `crates`,
`npm`, `pypi`, `docker`, `reddit`, `youtube`, `feed`, `sessions`,
`cli_tool`, `mcp_tool`. Each emits `SourceDocument` through the same pipeline.
Per-source walkthroughs live under [`docs/guides/`](docs/guides/):
[web](docs/guides/web-crawls.md), [local](docs/guides/local-sources.md),
[GitHub repos](docs/guides/github-repos.md),
[package registries](docs/guides/package-registries.md),
[sessions](docs/guides/sessions.md),
[CLI tool sources](docs/guides/cli-tool-sources.md),
[MCP tool sources](docs/guides/mcp-tool-sources.md).

- **`axon <source>`** — canonical acquisition. `--scope page|site|docs|repo|package|subreddit`,
  `--refresh if_stale|force|never`, `--watch disabled|ensure|enabled`,
  `--no-embed`, `--wait true`.
- **`axon scrape <url>`** — retained one-page projection: `scope=page`,
  `embed=true`, `limits.max_pages=1`, clean content output, no site fanout.
  Same pipeline as `axon <url> --scope page`.
- **`axon map <url>`** — discover URLs on a site without scraping (`embed=false`).
- **`axon sessions <export>`** — index Claude/Codex/Gemini session exports.

Vertical extractors auto-route known URLs (GitHub, PyPI, npm, crates.io, Reddit,
YouTube, …) to structured per-site extractors instead of generic HTML→markdown.
Disable with `AXON_ENABLE_VERTICALS=false` or the `[crawl.verticals]` config
section.

## Jobs, watches, and cleanup

**One durable job model** in SQLite owns lifecycle, attempts, stages, events,
heartbeats, artifacts, and provider reservations. Source jobs keep one job id
across resolve, acquire, prepare, embed, publish, graph, and cleanup. Job kinds:
`source`, `extract`, `watch`, `map`, `research`, `ask`, `query`, `retrieve`,
`memory`, `graph`, `prune`, `provider_probe`, `reset`.

```bash
axon <source> --wait true        # foreground: enqueue, run workers, block to terminal
axon <source>                    # detached: enqueue, return job id, exit
axon jobs list                   # all jobs
axon jobs get <job_id>           # status, stages, counts, errors
axon jobs events <job_id>        # paged event log
axon jobs stream                 # live event stream for consumers
axon jobs cancel <job_id>
axon jobs retry <job_id>
axon jobs recover                # reclaim stale running jobs (admin)
axon jobs cleanup                # remove old terminal jobs
axon jobs worker                 # run a standalone worker process
```

**Detached work needs workers.** A source/extract/watch/retry call returns a
job id when detached. A process must be running with workers for detached jobs
to advance — that's `axon serve` (or `jobs worker`). Use `--wait true` for
foreground completion.

**Watches** persist a canonical source request and schedule. Each due tick
leases the watch, enqueues one `source` job, and records the run.

```bash
axon watch create <source> --every 1d
axon watch list / get / status / update
axon watch exec <id>        # run one tick now
axon watch pause / resume / delete
axon watch history <id>
```

**Cleanup is plan-first.** `axon-prune` owns it; cleanup debt in the source
ledger records work that must be retried or reconciled.

```bash
axon prune plan <target>         # dry-run reviewable plan
axon prune exec <plan_id> --confirm   # destructive execution
axon reset plan                  # dry-run reset of local stores
axon reset exec --confirm        # destructive reset
```

`axon migrate --from <old> --to <new>` is the one-time upgrade path that scrolls
an unnamed-mode Qdrant collection and re-upserts it as named-mode (dense + BM42
sparse) for hybrid RRF search.

## Retrieval and analysis

See [`docs/guides/ask-query-retrieve-search.md`](docs/guides/ask-query-retrieve-search.md)
for how `ask`, `query`, `retrieve`, and `search` differ.

| Command | Purpose |
|---|---|
| `axon query <text>` | Hybrid vector search (RRF over dense + BM42 sparse) |
| `axon retrieve <url>` | Fetch a source's chunks/documents |
| `axon ask <question>` | RAG: retrieve relevant context, then answer with LLM |
| `axon chat <message>` | Direct LLM prompt (no retrieval) |
| `axon summarize <url>...` | Summarize one or more sources |
| `axon evaluate <question>` | RAG vs baseline with independent LLM judge |
| `axon suggest [focus]` | Suggest sources to index next |
| `axon train` | Collect human preference votes on RAG candidates |
| `axon search <query>` | Web search (SearXNG/Tavily), auto-queues source jobs |
| `axon research <query>` | Multi-source web research with LLM synthesis |
| `axon extract <url>...` | LLM-powered structured extraction (+ lifecycle) |
| `axon brand <url>` | Brand identity: colors, fonts, logos, favicon |
| `axon diff <a> <b>` | What changed between two URLs |
| `axon endpoints <url>` | Discover API endpoints from HTML/JS bundles |
| `axon screenshot <url>` | Full-page screenshot |

New Qdrant collections are created with named `dense` + `bm42` sparse vectors and
queried with Reciprocal Rank Fusion. Legacy unnamed collections fall back to
dense-only cosine; run `axon migrate` to upgrade, then point
`server.default-collection` at the new collection. Tune via `[search]` and
`[retrieval]` in `config.toml`.

## Memory

```bash
axon memory remember "..."        # store a memory record
axon memory list / search / show
axon memory link <a> <b>          # relate / supersede
axon memory supersede <old> <new>
axon memory context               # recall relevant memories for a query
```

Memory lives in its own Qdrant collection (`axon_memory`, or
`AXON_MEMORY_COLLECTION`) with a decay model: `base_score` blended from
semantic/confidence/salience/scope/reinforcement, multiplied by a
half-life decay unless pinned. The Claude plugin's `SessionStart` hook calls
`axon memory context` for session recall (gated by `AXON_SESSION_MEMORY_*`).

## CLI

The full command registry is **generated**, not hand-maintained, so it can't
drift. Headline groups:

```bash
axon <source>          # index a source (the canonical command)
axon scrape / map / sessions
axon query / ask / retrieve / search / research / summarize / evaluate
axon jobs / watch / prune / reset
axon memory / sources / domains / stats / status
axon serve / mcp / doctor / preflight / smoke / config
```

For the authoritative, always-current full registry — 110 commands across 49
groups with summaries and async/mutates markers — see the generated
[`docs/reference/cli/commands.md`](docs/reference/cli/commands.md) and the
machine-readable [`docs/reference/cli/commands.json`](docs/reference/cli/commands.json).
Regenerated by `cargo xtask schemas cli`; cross-checked against the live clap
tree. Per-command flags: `axon <cmd> --help`.

## MCP

Axon exposes **one** MCP tool named `axon`; actions route by `action` and
optional `subaction`. `axon mcp` defaults to stdio; `--transport http` (or
`both`) and `axon serve mcp` expose it over HTTP on the same listener as
`axon serve`.

```json
{ "action": "doctor" }
{ "action": "source", "source": "https://example.com", "scope": "site" }
{ "action": "ask", "query": "How does setup work?" }
{ "action": "query", "query": "embedding pipeline" }
{ "action": "jobs", "subaction": "events", "job_id": "<uuid>" }
{ "action": "watch", "subaction": "list" }
{ "action": "prune", "subaction": "plan" }
```

Response modes (`response_mode`): `path` (artifact ref), `inline`, `both`,
`auto_inline` — MCP responses never expose a server filesystem path; clients
follow the returned `artifact_id`. The generated wire contract is
[`docs/reference/mcp/tool-schema.md`](docs/reference/mcp/tool-schema.md).

HTTP auth: static bearer token (`AXON_HTTP_TOKEN`) or OAuth/lab-auth
(`AXON_AUTH_MODE=oauth`). Tokenless HTTP is allowed only for loopback
development binds; non-loopback binds require OAuth or a static token. `/mcp`
and `/v1/*` share the same auth policy.

## HTTP API and surfaces

`axon serve` starts one Axum HTTP server on `AXON_HTTP_HOST:AXON_HTTP_PORT`
(default `127.0.0.1:8001`) mounting:

- `/v1/*` — REST API (`/v1/sources`, `/v1/jobs`, `/v1/query`, `/v1/ask`,
  `/v1/memories`, `/v1/watches`, `/v1/prune/plan`, …). Async starts return
  `202` with `{ job_id, status, status_url }`.
- `/mcp` — MCP streamable HTTP.
- `/docs` — OpenAPI/Swagger UI (OpenAPI JSON at `/openapi.json`).
- The Aurora-styled web control panel (first-run setup, config/stack
  inspection, command runner).
- OAuth metadata + auth routes when `AXON_AUTH_MODE=oauth`.

Companion apps (each is a separate release component with its own README under
`apps/`): **Palette** (`apps/palette-tauri`, Tauri desktop app), **Android**
(`apps/android`, APK), **Chrome extension** (`apps/chrome-extension`), and the
bundled **web panel** (`apps/web`).

## LLM, search, and adapter configuration

**LLM synthesis** is selected by `AXON_LLM_BACKEND`:

| Backend | Setting |
|---|---|
| `gemini-headless` (default) | Gemini CLI under `~/.gemini` (OAuth) or `GEMINI_API_KEY` |
| `openai-compat` | `AXON_OPENAI_BASE_URL` (API root, not `/chat/completions`), `AXON_SYNTHESIS_OPENAI_MODEL` |
| `codex-app-server` | `codex app-server` over stdio in an isolated `CODEX_HOME`; `AXON_SYNTHESIS_CODEX_MODEL` |

Each backend has `AXON_SYNTHESIS_*_MODEL` and `AXON_CHAT_*_MODEL` overrides.
Completion concurrency/timeout are tuning knobs in `[providers.llm]`.

**Web search/research** uses a self-hosted SearXNG (`AXON_SEARXNG_URL`) when
configured, falling back to Tavily (`TAVILY_API_KEY`). Full-content vs.
snippet-only research is `providers.search.research-full-content` in
`config.toml`.

**Adapter credentials:** `GITHUB_TOKEN`, `GITLAB_TOKEN`, `GITEA_TOKEN`,
`REDDIT_CLIENT_ID`/`REDDIT_CLIENT_SECRET`, `HF_TOKEN`. These remain adapter
credentials even though every such target now enters through `SourceRequest`.

## Development

```bash
cargo build --bin axon                 # debug
cargo build --release --bin axon       # release
cargo check                            # fast type check
```

Test and lint:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features test-helpers -- -D warnings
cargo test --workspace --features test-helpers
```

`just` recipes (recommended): `just verify` (fmt-check + clippy + check + test),
`just fix` (fmt + clippy --fix), `just precommit` (full pre-PR gate),
`just watch-check` (check + test-lib on save).

Local dev infra (Qdrant/TEI/Chrome): `just services-up` / `just services-down`,
or directly `docker compose --env-file ~/.axon/.env -f docker-compose.yaml up -d`.
The dev compose file runs the locally built debug binary from `target/debug`
inside the `axon:dev-runtime` image — this is a dev convenience, not the axon
deployment path.

**Module layout:** Rust 2018+ file-per-module — no `mod.rs`. Module roots live
in `foo.rs`; submodules in `foo/bar.rs`. Tests live in sibling `foo_tests.rs`
sidecar files declared via `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;`
inside `foo.rs`. See `AGENTS.md` for the full convention.

**Monolith policy:** changed `.rs` files are enforced at CI and via lefthook
pre-commit — file size ≤ 500 lines (hard fail), function size warn at 80 / hard
fail at 120. Exemptions: `tests/**`, `benches/**`, `config/**`, `**/config.rs`,
and `**/*_tests.*`. `./scripts/install-git-hooks.sh` installs lefthook once.

## Release gates

Required before a production release:

- CI fmt / check / clippy / test.
- MCP schema doc sync (`python3 scripts/generate_mcp_schema_doc.py --check`).
- CLI help contract tests and generated registry drift checks.
- Docker image build + GHCR publish workflow.
- Compose smoke workflow.
- Self-hosted RTX 4070 smoke for Qwen3 TEI cold/warm timing.

Releases are per-component. Three of four (`palette`, `android`, `chrome`) are
release-please-managed; `cli` is bumped manually with
`cargo xtask bump-version cli patch|minor|major` (release-please can't handle
`version.workspace = true`). The PR gate is:

```bash
cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
```

See `AGENTS.md` for the full release pipeline and version-bearing-file rules.

## Troubleshooting

Fast checks:

```bash
# Bare-metal
axon preflight
axon doctor
systemctl status axon
journalctl -u axon -f

# Incus
incus exec axon -- axon doctor
incus exec axon -- systemctl status axon-native
incus exec axon -- journalctl -u axon-native -f
```

Important paths:

- Bare-metal: `~/.axon/` (`.env`, `config.toml`, `jobs.db`, `logs/`,
  `artifacts/`, `output/`, `screenshots/`).
- Incus: `/mnt/axon-data/` inside the container ↔ `~/.axon-incus/` on the host.
  Host-native CLI commands do **not** see the Incus deployment's data, and vice
  versa — a deliberate, permanent fork (see `deploy/incus/README.md`).

Common failures:

- **GPU unavailable:** verify `nvidia-smi` and NVIDIA Container Toolkit on the
  TEI host. Under Incus, the `nvidia-procfs` device may need re-adding after a
  container restart (see `deploy/incus/README.md`).
- **TEI slow on first boot:** model download/cache warmup is the cold path.
- **LLM unauthenticated:** for Gemini, run the Gemini CLI login outside Axon;
  for OpenAI-compatible, set `AXON_LLM_BACKEND=openai-compat` +
  `AXON_OPENAI_BASE_URL` + `AXON_SYNTHESIS_OPENAI_MODEL`.
- **Detached jobs never advance:** no worker is running — start `axon serve`
  (or `axon jobs worker`).
- **Auth failures:** make sure Claude/plugin config uses the same token as
  `AXON_HTTP_TOKEN` in `~/.axon/.env`.

## Documentation

Beyond this README, the live doc tree (under [`docs/`](docs/)):

**Guides** ([`docs/guides/`](docs/guides/)) — [quickstart](docs/guides/quickstart.md),
[getting-started](docs/guides/getting-started.md),
[configuration](docs/guides/configuration.md), and per-source walkthroughs
([web](docs/guides/web-crawls.md), [local](docs/guides/local-sources.md),
[GitHub](docs/guides/github-repos.md),
[packages](docs/guides/package-registries.md), [sessions](docs/guides/sessions.md),
[CLI tools](docs/guides/cli-tool-sources.md),
[MCP tools](docs/guides/mcp-tool-sources.md),
[ask/query/retrieve/search](docs/guides/ask-query-retrieve-search.md)).

**Operations** ([`docs/operations/`](docs/operations/)) —
[deployment guide](docs/operations/deployment.md),
[operations runbook](docs/operations/operations.md),
[security model](docs/operations/security.md),
[performance tuning](docs/operations/performance.md).

**Contributing** ([`docs/development/`](docs/development/)) —
[contributing](docs/development/contributing.md),
[adding a source adapter](docs/development/adding-source-adapter.md),
[adding an MCP action](docs/development/adding-mcp-action.md),
[adding a REST route](docs/development/adding-rest-route.md).

**Architecture** — [`docs/architecture/overview.md`](docs/architecture/overview.md).

**Generated references** ([`docs/reference/`](docs/reference/)) — the
machine-readable source of truth, regenerated by `cargo xtask schemas`:
[CLI command registry](docs/reference/cli/commands.json),
[OpenAPI](docs/reference/rest/openapi.json),
[MCP tool schema](docs/reference/mcp/tool-schema.json),
[API DTOs](docs/reference/api/schemas.json),
[config/env schemas](docs/reference/config/config.schema.json),
[database schema](docs/reference/runtime/database-schema.json),
[graph](docs/reference/sources/graph.schema.json),
[vector payload](docs/reference/sources/vector-payload.schema.json),
[events](docs/reference/runtime/events.schema.json),
[provider capabilities](docs/reference/runtime/provider-capabilities.schema.json).

**Per-crate agent contracts** — `crates/<name>/src/CLAUDE.md` (with
`AGENTS.md`/`GEMINI.md` symlinks) for each workspace crate.

**Design contract packet** — [`docs/pipeline-unification/`](docs/pipeline-unification/README.md)
is the historical design record for the source-pipeline unification effort
(issue #298). It's retained as a permanent archive; the live docs above
supersede it for current usage.

## Related files

- `install.sh` / `install.ps1` — verified one-line installers.
- `deploy/incus/` — Incus deployment (profile, bootstrap, host unit).
- `deploy/systemd/` — bare-metal systemd unit + walkthrough.
- `docker-compose.prod.yaml` — canonical infra reference (Qdrant/TEI/Chrome
  image versions and ports); also the dev stack base.
- `.env.example` / `config.example.toml` — env and tuning templates.
- `plugins/axon/.claude-plugin/plugin.json` — Claude plugin manifest.
- `docs/reference/cli/commands.md` / `.json` — generated CLI command registry.
- `docs/reference/mcp/tool-schema.md` — generated MCP wire contract.
- `docs/` — full documentation tree: guides, `reference/`, architecture,
  operations, and the `pipeline-unification/` design contract packet.

## Related servers

- [soma](https://github.com/jmagar/soma) — RMCP runtime for provider-backed MCP servers.
- [unifi-rmcp](https://github.com/jmagar/unifi-rmcp) — UniFi controller REST API bridge.
- [tailscale-rmcp](https://github.com/jmagar/tailscale-rmcp) — Tailscale API bridge.
- [unraid-rmcp](https://github.com/jmagar/unraid-rmcp) — Unraid GraphQL bridge.
- [apprise-rmcp](https://github.com/jmagar/apprise-rmcp) — Apprise notification fan-out.
- [gotify-rmcp](https://github.com/jmagar/gotify-rmcp) — Gotify push notification bridge.
- [arcane-rmcp](https://github.com/jmagar/arcane-rmcp) — Arcane Docker management bridge.
- [yarr](https://github.com/jmagar/yarr) — media-stack bridge (Sonarr/Radarr/Prowlarr/Plex).
- [ytdl-rmcp](https://github.com/jmagar/ytdl-rmcp) — media download and metadata workflow.
- [synapse-rmcp](https://github.com/jmagar/synapse-rmcp) — local Synapse workflow server.
- [cortex](https://github.com/jmagar/cortex) — syslog and homelab log aggregation.
- [labby](https://github.com/jmagar/labby) — homelab control plane and MCP gateway.
- [lumen](https://github.com/jmagar/lumen) — local semantic code search.

## License

MIT
