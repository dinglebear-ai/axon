# Axon Architecture
Last Modified: 2026-05-06

Version: 1.0.0
Last Updated: 01:26:53 | 02/25/2026 EST

## Table of Contents

1. [Purpose and Scope](#purpose-and-scope)
2. [System Context](#system-context)
3. [Runtime Components](#runtime-components)
4. [Execution Entry Points](#execution-entry-points)
5. [CLI and Config Flow](#cli-and-config-flow)
6. [Crawl and Content Pipeline](#crawl-and-content-pipeline)
7. [Async Job Architecture](#async-job-architecture)
8. [Vector and RAG Pipeline](#vector-and-rag-pipeline)
9. [Ingest Pipeline](#ingest-pipeline)
10. [Web Runtime Architecture](#web-runtime-architecture)
11. [Omnibox and Pulse Flows](#omnibox-and-pulse-flows)
12. [Data Model and Persistence](#data-model-and-persistence)
13. [Configuration Resolution](#configuration-resolution)
14. [Failure Handling and Recovery](#failure-handling-and-recovery)
15. [End-to-End Flows](#end-to-end-flows)
16. [Key Source Map](#key-source-map)

## Purpose and Scope

This document defines the current architecture of `axon_rust` across:

- CLI command execution and dispatch
- Crawl/extract/embed/ingest asynchronous pipelines
- Vector storage and retrieval (Qdrant + TEI)
- Web runtimes (`serve` websocket/download bridge and `apps/web` Next.js UI)
- Omnibox/pulse interaction and data flow

It supersedes the previous omnibox-only architecture note.

## System Context

```mermaid
flowchart LR
  U[User or API client]
  CLI[axon CLI binary]

  QD[(Qdrant)]
  TEI[TEI embeddings]
  LLM[OpenAI-compatible API]
  CHR[Chrome/CDP]
  SQ[(SQLite jobs)]

  U --> CLI

  CLI --> QD
  CLI --> TEI
  CLI --> LLM
  CLI --> CHR
  CLI --> SQ
```

## Runtime Components

| Component | Role |
|---|---|
| `main.rs` + `lib.rs` | Binary entry and top-level command loop/dispatch |
| `crates/cli/*` | Command handlers and subcommand routing |
| `crates/core/*` | Config parsing, HTTP safety, content transforms, logging |
| `crates/crawl/*` | Crawl engine, render mode strategy, sitemap backfill |
| `crates/jobs/*` | SQLite-backed worker runtime + job state transitions |
| `crates/vector/*` | Embed/query/retrieve/ask/evaluate/suggest operations |
| `crates/services/acp/*` | ACP session lifecycle, subprocess bridge, runtime state, permission map |
| `config/docker-compose.services.yaml` | Self-hosted infrastructure services (Qdrant, TEI, Chrome) |

## Execution Entry Points

```mermaid
flowchart TD
  A[main.rs] --> B[axon::run in lib.rs]
  B --> C{--cron-every-seconds?}
  C -->|no| D[run_once]
  C -->|yes| E[cron loop -> run_once]
  D --> F{CommandKind}
  F --> G[CLI command handler]
```

- `main.rs` loads `.env` and invokes `axon::run`.
- `lib.rs` owns run-loop concerns (logging init, optional cron, dispatch to handlers).
- Command dispatch is centralized in `run_once` using `CommandKind`.

## CLI and Config Flow

```mermaid
sequenceDiagram
  participant User
  participant Clap as clap parser
  participant Parse as parse_args/into_config
  participant Config as Config struct
  participant Cmd as command handler

  User->>Clap: axon <command> [flags]
  Clap->>Parse: parsed CLI args
  Parse->>Parse: env + flag merge
  Parse->>Parse: apply performance profile
  Parse->>Parse: normalize local service URLs
  Parse->>Config: fully-resolved Config
  Config->>Cmd: shared runtime config
```

Key points:

- Argument schema is defined in `crates/core/config/cli.rs` and `crates/core/config/cli/global_args.rs`.
- Parsing/normalization is in `crates/core/config/parse.rs`.
- Effective runtime settings are stored in `crates/core/config/types/config.rs::Config`.
- URL seed handling is consolidated in `crates/cli/commands/common.rs` (`parse_urls`, `start_url_from_cfg`).

## Crawl and Content Pipeline

```mermaid
flowchart TD
  A[Seed URLs] --> B[validate_url + SSRF checks]
  B --> C{Render mode}
  C -->|http| D[crawl_raw]
  C -->|chrome| E[crawl with CDP]
  C -->|auto-switch| F[HTTP first then fallback heuristic]

  D --> G[collect pages]
  E --> G
  F --> G

  G --> H[HTML -> markdown transform]
  H --> I[thin-page filtering]
  I --> J[sitemap backfill]
  J --> K[manifest + output files]
  K --> L{embed enabled?}
  L -->|yes| M[queue/embed now]
  L -->|no| N[store output only]
```

Key responsibilities:

- HTTP safety, SSRF guarding, and client setup in `crates/core/http.rs`.
- Content transformation and markdown extraction in `crates/core/content.rs`.
- Crawl orchestration in `crates/crawl/engine.rs`.
- Auto-switch mode evaluates crawl quality and can rerun with Chrome.
- Sitemap backfill extends coverage beyond direct traversal.

### Map Command

`map` consumes a unified URL set from the crawl engine (`map_with_sitemap` in `crates/crawl/engine.rs`).
The CLI no longer merges or deduplicates sitemap URLs itself — the engine owns the full URL set with
deterministic sort+dedup before returning `MapResult`. This keeps the CLI handler as a thin
delegation layer and ensures the output contract is tested at the engine level.

## Async Job Architecture

Jobs are persisted in SQLite (lite mode). Workers run in-process within the same tokio runtime.

```mermaid
flowchart LR
  ENQ[enqueue command] --> SQ[(insert pending row in SQLite)]
  SQ --> WK[in-process worker]
  WK --> CLM[claim pending row]
  CLM --> RUN[set running + started_at]
  RUN --> PROC[process job]
  PROC --> DONE[set completed + result_json]
  PROC --> FAIL[set failed + error_text]
```

State model:

- Shared statuses in `crates/jobs/status.rs`: `pending`, `running`, `completed`, `failed`, `canceled`.
- Atomic claim/fail/update helpers in `crates/jobs/common/job_ops.rs`.
- Worker lane orchestration in `crates/jobs/worker_lane.rs`.
- Stale job watchdog in `crates/jobs/common/watchdog.rs`.

Job families:

- Crawl: `crates/jobs/crawl/runtime/worker/loops.rs` (own polling loop — see Worker Architecture below)
- Extract: `crates/jobs/extract/worker.rs` (uses `worker_lane.rs`)
- Embed: `crates/jobs/embed/worker.rs` (uses `worker_lane.rs`)
- Ingest (unified `axon ingest <target>`): `crates/jobs/ingest/process.rs` (uses `worker_lane.rs`; target auto-detected by `crates/ingest/classify.rs`)

### Worker Architecture

#### Generic Worker Lane (worker_lane.rs)

`worker_lane.rs` provides a generic polling consumer loop shared by:
- Embed worker (`crates/jobs/embed/worker.rs`)
- Extract worker (`crates/jobs/extract/worker.rs`)
- Ingest worker (`crates/jobs/ingest/process.rs`)

Each worker type creates N lanes (configurable via `AXON_*_LANES` env vars).
Each lane processes jobs sequentially.

#### Why the Crawl Worker Doesn't Use worker_lane.rs

The crawl worker has its own loop in `crates/jobs/crawl/runtime/worker/loops.rs`.

**Root cause**: `spider.rs` futures are `!Send`. They cannot be:
- Spawned with `tokio::spawn()` (requires `Send + 'static`)
- Moved across thread boundaries (including `FuturesUnordered`)

The crawl worker works around this by pinning futures with `tokio::pin!()` and
polling them inside a `select!` loop on a single task. This preserves the
1-job-per-lane guarantee while keeping the non-Send future alive on the same thread.

## Vector and RAG Pipeline

```mermaid
flowchart TD
  A[markdown/text input] --> B[chunk_text]
  B --> C[tei_embed batches]
  C --> D{TEI response}
  D -->|ok| E[qdrant_upsert points]
  D -->|413/429/503| F[split/retry with backoff]
  F --> C

  Q[query/ask/evaluate] --> R[qdrant search]
  R --> S[ranking + candidate selection]
  S --> T[context assembly]
  T --> U[LLM completion]
```

Key behaviors:

- Embedding implementation in `crates/vector/ops/tei.rs`.
- Qdrant operations and collection lifecycle in `crates/vector/ops/qdrant/*`.
- Command-level vector flows in `crates/vector/ops/commands/*`.
- Ingest sources eventually call vector embedding paths so all content lands in Qdrant with metadata.

## Ingest Pipeline

### Unified Ingest Entry Point (v0.12.0)

`axon ingest <target>` replaces the three separate `github`, `reddit`, and `youtube` CLI commands. `crates/ingest/classify.rs` auto-detects the source type from the target string:

```mermaid
flowchart TD
  A[axon ingest <target>] --> B[classify_target]
  B -->|r/ prefix or reddit.com| C[IngestSource::Reddit]
  B -->|@handle / known YT host / 11-char ID| D[IngestSource::YouTube]
  B -->|github.com or owner/repo| E[IngestSource::GitHub]
  C --> F[crates/ingest/reddit.rs]
  D --> G[crates/ingest/youtube.rs]
  E --> H[crates/ingest/github.rs]
  F --> I[embed_prepared_docs -> Qdrant]
  G --> I
  H --> I
```

Detection order: Reddit → YouTube → GitHub (first match wins).

### Ingest Submodule Layout

```text
crates/ingest/
├── classify.rs          # auto-detection: classify_target()
├── github.rs            # module root
├── github/
│   ├── files.rs         # file tree fetch + raw content
│   ├── issues.rs        # octocrab paginated issues + PRs
│   ├── meta.rs          # gh_* structured metadata for Qdrant points (v0.12.0)
│   └── wiki.rs          # git clone --depth=1 wiki
├── reddit.rs            # module root
├── reddit/
│   ├── client.rs        # OAuth2 client credentials
│   ├── comments.rs      # recursive comment tree
│   ├── meta.rs          # reddit_* structured metadata for Qdrant points (v0.12.0)
│   └── types.rs         # Reddit API response types
├── youtube.rs           # module root
├── youtube/
│   ├── meta.rs          # yt_* structured metadata for Qdrant points (v0.12.0)
│   └── vtt.rs           # parse_vtt_to_text: yt-dlp VTT transcript parser
└── sessions.rs          # AI session export ingest
```

### MCP Artifacts Module (`crates/mcp/server/artifacts/`)

Added in v0.12.0 to manage MCP tool response artifacts:

| File | Responsibility |
|---|---|
| `artifacts.rs` | Module root; `ArtifactStore` type |
| `artifacts/lifecycle.rs` | Create, expire, and garbage-collect artifacts |
| `artifacts/path.rs` | Artifact path resolution and URL generation |
| `artifacts/respond.rs` | Build MCP tool response payloads embedding artifact refs |
| `artifacts/shape.rs` | `ArtifactShape` enum: `Blob`, `Text`, `Json`, `Image` |

### ACP Service (`crates/services/acp/`)

`services/acp.rs` (formerly 2060-line monolith) was split (v0.11.2) into:

| File | Responsibility |
|---|---|
| `bridge.rs` | `AcpBridgeClient` — implements the ACP SDK `Client` trait; forwards session notifications and permission requests through the service event channel. Owns `AcpRuntimeState` (RefCell-based, single-threaded via `LocalSet`). |
| `runtime.rs` | `run_prompt_turn`, `run_session_probe` — one-shot orchestration. Contains `AdapterGuard` RAII cleanup. |
| `session.rs` | Subprocess spawn, connection init, session setup sub-functions called from `runtime.rs`. |
| `adapters.rs` | Adapter detection (`is_codex_adapter`, `is_gemini_adapter`) and model override injection. |
| `config.rs` | Config option discovery and model cache readers (Codex `models_cache.json`, Gemini `settings.json`). |
| `mapping.rs` | SDK event → service type conversions. |
| `persistent_conn.rs` | `AcpConnectionHandle` — single persistent adapter handle used by Pulse Chat sessions. |

Key runtime patterns:

- `AcpRuntimeState` uses `RefCell` (not `Mutex`) — single-threaded via `LocalSet` on a `current_thread` runtime inside `spawn_blocking`. Safe; compiler enforces via `?Send` bounds.
- Permission map uses `DashMap` (lock-free concurrent reads for session-scoped permission routing, SEC-7).
- `AdapterGuard` — RAII guard that cleans up the ACP subprocess on `Drop`, preventing zombie processes.
- `select! { biased; }` — event-drain loop ordering ensures cancellation signals are checked before new input, preventing unbounded queuing on client disconnect.

## Data Model and Persistence

Primary tables (SQLite, auto-created via `ensure_schema()`):

- `axon_crawl_jobs`
- `axon_extract_jobs`
- `axon_embed_jobs`
- `axon_ingest_jobs`

Common columns:

- `id`, `status`, `created_at`, `updated_at`, `started_at`, `finished_at`, `error_text`, `config_json`, `result_json`

Ingest-specific discriminator:

- `source_type` + `target` replace URL-based identifiers.

Storage responsibilities:

- SQLite: job metadata and lifecycle state
- Qdrant: vector points + retrieval corpus

## Configuration Resolution

```mermaid
flowchart LR
  CLI[CLI flags] --> CFG[Config resolution]
  ENV[Environment variables] --> CFG
  PROF[Performance profile defaults] --> CFG
  DOCKER[Docker/local URL normalization] --> CFG
  CFG --> HANDLERS[all commands/workers]
```

Important behavior:

- Container DNS endpoints are normalized for local execution when needed.
- Profiles (`high-stable`, `balanced`, `extreme`, `max`) apply batch, timeout, retry, and concurrency defaults.
- Queue names and collection names are centrally configurable.

## Failure Handling and Recovery

Resilience patterns implemented:

- Atomic row claiming prevents duplicate worker ownership.
- Watchdog can reclaim stale `running` jobs.
- Embedding retries handle transient TEI overload and payload limits.
- Command/output streams include typed error events over websocket.
- Job subcommands (`status`, `errors`, `list`, `recover`, `cancel`) provide operational control.

## End-to-End Flows

### 1) Crawl with Async Queue

1. User runs `axon crawl <url>` (default async).
2. Command inserts `pending` job row and publishes job id to queue.
3. Worker claims row, marks `running`, executes crawl.
4. Results and artifacts are written, optional embedding happens.
5. Job row is finalized with `completed` or `failed`.

### 2) Ask/RAG Query

1. User runs `axon ask <question>` or pulse sends a chat request.
2. Query retrieves candidates from Qdrant.
3. Ranking/context assembly builds prompt context.
4. LLM endpoint generates final answer.

## Key Source Map

Core runtime:

- `main.rs`
- `lib.rs`
- `crates/core/config/cli.rs`
- `crates/core/config/cli/global_args.rs`
- `crates/core/config/parse.rs`
- `crates/core/config/types/config.rs`
- `crates/core/config/types.rs`
- `crates/core/http.rs`
- `crates/core/content.rs`

Crawl/jobs/vector:

- `crates/crawl/engine.rs`
- `crates/jobs/status.rs`
- `crates/jobs/common/job_ops.rs`
- `crates/jobs/worker_lane.rs`
- `crates/jobs/crawl/runtime/worker/loops.rs`
- `crates/jobs/extract/worker.rs`
- `crates/jobs/embed/worker.rs`
- `crates/jobs/ingest/process.rs`
- `crates/vector/ops.rs`
- `crates/vector/ops/tei.rs`

Ingest:

- `crates/ingest/classify.rs`
- `crates/ingest/github.rs` + `crates/ingest/github/` (files, issues, meta, wiki)
- `crates/ingest/reddit.rs` + `crates/ingest/reddit/` (client, comments, meta, types)
- `crates/ingest/youtube.rs` + `crates/ingest/youtube/` (meta, vtt)
- `crates/ingest/sessions.rs`

ACP + MCP:

- `crates/services/acp.rs`
- `crates/services/acp/bridge.rs`
- `crates/services/acp/runtime.rs`
- `crates/services/acp/session.rs`
- `crates/services/acp/adapters.rs`
- `crates/services/acp/config.rs`
- `crates/services/acp/mapping.rs`
- `crates/services/acp/persistent_conn.rs`
- `crates/mcp/server/artifacts.rs`
- `crates/mcp/server/artifacts/` (lifecycle, path, respond, shape)

## Security: Destructive Operations

The following CLI operations are **unauthenticated** — any process with access to
the SQLite database can invoke them:

- `axon crawl clear` — deletes ALL crawl jobs
- `axon extract clear` — deletes ALL extract jobs
- `axon crawl cancel <id>` — cancels a specific job

**Accepted risk**: Axon is a self-hosted single-user tool. The SQLite database is a
local file. Qdrant is bound to `127.0.0.1` (or internal Docker network). External
exposure is prevented at the infrastructure layer (Docker port mappings, Tailscale ACLs).

---

If this architecture changes, update this file in the same PR as the behavior change.
