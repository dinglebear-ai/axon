# Current Implementation Sweep
Last Modified: 2026-06-30

## Contract

This file records the read-only sweep of the current Axon implementation that
informs the pipeline-unification contracts. It is not a compatibility promise.
It exists so implementation work starts from the real current surface and
deletes/replaces old paths deliberately.

## Workspace Members Today

Current Cargo workspace members:

| Crate | Current Role |
|---|---|
| `axon` | Root binary/bootstrap crate. |
| `axon-api` | Current shared DTOs, MCP schema pieces, job DTO/status/progress, ingest/diff/purge/explain result types. |
| `axon-authz` | Current OAuth scope constants and HTTP auth helpers. |
| `axon-core` | Config, HTTP/content helpers, artifacts, redaction, LLM provider implementations, CLI config parsing. |
| `axon-crawl` | Spider.rs crawl engine, scrape helpers, sitemap/backfill, Chrome bootstrap. |
| `axon-extract` | Vertical extractor framework, scrape dispatch, deterministic/LLM structured extraction sync path. |
| `axon-ingest` | GitHub, GitLab, Gitea/Forgejo, generic Git, Reddit, RSS, YouTube, and session ingest. |
| `axon-vector` | SourceDocument planning, chunking, TEI embedding, Qdrant collection/upsert/search/delete, hybrid retrieval, ask/evaluate/query/retrieve/suggest. |
| `axon-code-index` | Local code-search ledger, manifest diff, generations, leases, progress, freshness, and code-search indexing. |
| `axon-jobs` | SQLite job tables/runtime/workers/watch scheduler, heartbeats, watchdog, panic guard, starvation detector. |
| `axon-services` | Typed service facade/orchestration consumed by CLI, MCP, REST, and app surfaces. |
| `axon-mcp` | Single-tool MCP server with action/subaction routing and MCP HTTP auth. |
| `axon-web` | Axum REST server, web panel, OpenAPI docs, SSE/chat routes, mobile sessions, artifacts. |
| `axon-cli` | Command dispatch, command handlers, human/JSON rendering, completions. |
| `xtask` | Current developer automation crate. |

Current `xtask` automation includes focused checks such as API parity
generation/checking, OpenAPI drift checks, Android route-contract checks, CLI
help contract checks, and CLAUDE/AGENTS/GEMINI symlink checks. The target
`xtask docs ...` and aggregate `xtask schemas ...` command tree does not exist
yet.

Target crates introduced by this contract do not exist yet:

- `axon-error`
- `axon-observe`
- `axon-route`
- `axon-adapters`
- `axon-ledger`
- `axon-parse`
- `axon-graph`
- `axon-memory`
- `axon-document`
- `axon-embedding`
- `axon-vectors`
- `axon-retrieval`
- `axon-llm`
- `axon-prune`

## Current CLI Surface

Current CLI commands are parsed in `crates/axon-core/src/config/cli.rs`.

Current top-level commands include:

- web/extraction: `scrape`, `crawl`, `watch`, `monitor`, `map`, `endpoints`,
  `extract`, `search`, `research`, `brand`, `diff`, `screenshot`
- indexing/RAG: `embed`, `query`, `code-search`, `code-search-watch`,
  `retrieve`, `ask`, `summarize`, `evaluate`, `train`, `suggest`, `sources`,
  `domains`, `stats`, `dedupe`, `purge`, `refresh`, `fresh`
- external sources/imports: `ingest`, `memory`, `sessions`
- runtime/setup: `status`, `debug`, `doctor`, `completions`, `serve`,
  `preflight`, `smoke`, `compose`, `setup`, `mcp`, `migrate`, `config`,
  `sync`, `update`, `palette`

Clean-break implications:

- remove `code-search-watch` from the normal public command model
- remove purge aliases such as `delete-url`/`delete`
- collapse source acquisition/indexing into `axon <source>` plus explicit
  actions where they remain semantically distinct
- retain `extract` as the structured LLM extraction command, not an indexing
  category
- keep `map` as an explicit command/action/endpoint

## Current REST Surface

Current REST routes are registered in `crates/axon-web/src/server/routing.rs`.

Read-scoped routes:

- `GET /v1/capabilities`
- `GET /v1/sources`
- `GET /v1/domains`
- `GET /v1/stats`
- `GET /v1/status`
- `GET /v1/doctor`
- `GET /v1/collections`
- `GET /v1/mobile/sessions`
- `GET /v1/mobile/sessions/{id}`
- `POST /v1/query`
- `POST /v1/retrieve`
- `POST /v1/map`
- `GET /v1/artifacts`
- `GET /v1/artifacts/{*path}`

Write-scoped routes:

- `POST /v1/endpoints`
- `POST /v1/brand`
- `POST /v1/diff`
- `POST /v1/screenshot`
- `POST /v1/ask`
- `POST /v1/ask/stream`
- `POST /v1/chat`
- `POST /v1/chat/stream`
- `POST /v1/evaluate`
- `POST /v1/suggest`
- `POST /v1/scrape`
- `POST /v1/summarize`
- `POST /v1/summarize/stream`
- `POST /v1/search`
- `POST /v1/research`
- `POST /v1/research/stream`
- `POST /v1/memory`
- nested job routes under `/v1/crawl`, `/v1/embed`, `/v1/extract`,
  `/v1/ingest`
- `POST /v1/dedupe`
- `POST /v1/purge`
- `GET/POST /v1/watch`
- `POST /v1/watch/{id}/run`
- nested prepared-session ingest under `/v1/ingest/sessions/prepared`
- `PUT/DELETE /v1/mobile/sessions/{id}`

Removed/not exposed today:

- `POST /v1/actions` returns removed/not found
- `POST /v1/migrate` returns not exposed over REST

Clean-break implications:

- direct REST routes should remain direct routes, not `/v1/actions`
- source acquisition/indexing routes should converge under `/v1/sources`
- `map` remains a top-level route/action
- mobile/session/memory/artifact surfaces must be represented in the end-state
  REST contract

## Current MCP Surface

Current MCP uses one tool named `axon` with `action` and optional `subaction`.
It also exposes the `axon_status_dashboard` MCP Apps/widget tool as a dashboard
presentation helper, not as a second operation surface.

Direct actions include:

- `ask`
- `code_search`
- `map`
- `endpoints`
- `query`
- `research`
- `retrieve`
- `scrape`
- `screenshot`
- `search`
- `summarize`
- `brand`
- `diff`
- `evaluate`
- `suggest`
- `elicit_demo`
- `purge`
- `memory`
- `vertical_scrape`

Lifecycle action families:

- `crawl`: `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover`
- `extract`: `start`, `status`, `cancel`, `list`, `cleanup`, `clear`,
  `recover`
- `embed`: `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover`
- `ingest`: `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover`

Info actions:

- `doctor`
- `domains`
- `help`
- `sources`
- `stats`
- `status`

Clean-break implications:

- keep one MCP tool, not one tool per operation
- remove old action aliases and legacy action families from generated schemas
- add source/memory/jobs/provider/graph surfaces explicitly through the shared
  DTO/action registry

## Current Source And Indexing Paths

### Crawl

`axon-crawl` owns Spider.rs crawling, sitemap discovery/backfill, render mode
selection, adaptive concurrency, crawl memory guards, WARC/automation support,
and per-page crawl progress. Crawl jobs are durable SQLite jobs today.

### Scrape And Vertical Extraction

`axon-extract` owns vertical extractor dispatch and generic scrape output.
`services::scrape` can route matching URLs through vertical extractors before
generic HTTP/markdown scraping. Sync structured extraction also lives here.

### Embed

`axon-vector` owns the current normalized post-acquisition path:

```text
SourceDocument
  -> prepare_source_document
  -> PreparedDoc
  -> embed_prepared_docs
  -> TEI
  -> Qdrant
```

Current local directory embed routes code-like files through
`SourceDocument::try_new_file(SourceOrigin::LocalFile, ...)` and tree-sitter
chunking when supported.

### Ingest

`axon-ingest` owns external source ingestion:

- GitHub: metadata, files, issues, PRs, wiki
- GitLab: metadata, files, issues, merge requests, wiki
- Gitea/Forgejo: metadata, files, issues, pull requests
- generic HTTPS Git: files
- Reddit: subreddit/thread posts and comments
- RSS/Atom/JSON feeds
- YouTube: video/playlist/channel transcripts and metadata through `yt-dlp`
- sessions: Claude/Codex/Gemini exports

Most ingest paths already build `SourceDocument` or use planner helpers before
calling the shared embedding pipeline.

### Local Code Search

`axon-code-index` owns the strongest current mutable-source model:

- project identity and allowed roots
- manifest diffing
- SQLite store/schema
- generations
- leases
- freshness checks
- progress events
- cleanup summary

Clean-break implication: generalize these patterns into `axon-ledger`,
`axon-jobs`, `axon-observe`, and `axon-prune`; do not preserve
`code-search-watch` as a public command.

### Memory

Memory is currently service-owned with SQLite metadata and Qdrant content. The
target split promotes it to `axon-memory` with explicit decay/review/context
contracts.

## Current Job Runtime

`axon-jobs` currently has separate job kinds/tables:

- crawl: `axon_crawl_jobs`
- embed: `axon_embed_jobs`
- extract: `axon_extract_jobs`
- ingest: `axon_ingest_jobs`

Current runtime strengths to preserve:

- SQLite-only runtime; no external broker
- enqueue-only vs worker-spawning construction split
- per-job heartbeat guard
- startup and periodic stale running job recovery
- panic guard so a job panic fails the job without killing the lane
- starvation detector for pending jobs with no running lane
- cancellation tokens and safe interruption points
- bounded channels

Target implication: move to one durable job model, but carry forward these
operational behaviors.

## Current Retrieval And Vector Behavior

`axon-vector` currently combines responsibilities that target contracts split:

- `axon-document`: source planning and chunking
- `axon-embedding`: TEI embedding and batch/retry behavior
- `axon-vectors`: Qdrant collection, payload, upsert, delete, search
- `axon-retrieval`: hybrid search, ranking, context assembly, citations
- `axon-llm`: currently in `axon-core` for synthesis providers

Important current behavior to preserve:

- named dense + sparse BM42 collections for new Qdrant collections
- GET-first collection creation and VectorMode cache
- TEI 413 auto-splitting and 429/5xx retry
- pooled embedding groups and doc-level failure accounting
- upsert-first then stale-tail cleanup
- Qdrant facet use for aggregate URL/domain counts
- source-doc planner owns normalized payload fields and strips duplicates
- tree-sitter code chunking for Rust, Python, JavaScript, TypeScript/TSX, Go,
  Bash, JSON, YAML, and TOML with prose fallback
- hybrid RRF retrieval plus lexical/ranking/citation pipeline

## Current App Surfaces

- `apps/web` is a real built and embedded web panel surface served by
  `axon-web`; it is not just a future target.
- `apps/palette-tauri` is a real desktop target and must be covered by the
  palette contract. It currently has generated API client plumbing from the
  OpenAPI output.
- Current REST includes mobile-session routes; `apps/android` is a real target
  surface and currently uses generated and hand-written client wrappers around
  the existing REST surface.
- `apps/chrome-extension` is an implemented MV3 extension surface today. The
  clean-break chrome-extension contract must keep it on shared REST/source
  capture contracts rather than letting it own ingestion.
- `apps/desktop` exists as an empty/placeholder directory today; the real
  desktop app surface is currently `apps/palette-tauri`.

## Sweep Conclusions

- The target crate split is valid, but implementation must preserve current
  operational details instead of flattening them away.
- `axon-jobs` must not depend on `axon-services`; composition should inject
  worker functions or service closures into the runtime.
- `axon-retrieval` should depend on `EmbeddingProvider`/`VectorStore` traits,
  not concrete TEI/Qdrant clients.
- Existing `axon-vector` behavior should be split carefully because it holds
  several correctness and latency-critical behaviors.
- Existing `axon-ingest` coverage is broader than the initial target examples;
  GitLab, Gitea/Forgejo, generic Git, RSS, and sessions must be first-class in
  the adapter/scopes/new-source contracts.
