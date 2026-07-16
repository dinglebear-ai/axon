# Surface Removal Contract
Last Modified: 2026-06-30

## Contract

Removed CLI commands, MCP actions, REST routes, DTO fields, config keys, help
entries, generated clients, and docs are deleted as part of the clean break.

There are no compatibility aliases and no public deprecation/tombstone window.
This is a one-consumer app; correctness and simplicity matter more than
preserving old user-facing surfaces.

Exception: `axon scrape <url>` is retained as a canonical CLI convenience for
one-page web acquisition. It is not a compatibility alias: it must construct a
`SourceRequest` with `scope=page` and `embed=true`, then run through the same
web adapter, ledger, document preparation, embedding, vector publish, and
cleanup path as `axon <url> --scope page`.

## Design Rules

- Delete removed parser variants.
- Delete removed MCP action variants.
- Delete removed REST routes.
- Delete removed OpenAPI entries.
- Delete removed generated client operations.
- Delete removed help output.
- Delete removed docs/examples.
- Delete old code paths after canonical replacements are implemented.
- Do not keep aliases.
- Do not keep hidden shims.
- Do not execute legacy behavior through remap code.
- Retained convenience commands must be projections over canonical DTOs and
  services, not alternate data paths.

## Removed CLI Commands

| Removed | Canonical Replacement |
|---|---|
| `axon embed <source>` | `axon <source>` |
| `axon ingest <source>` | `axon <source>` |
| `axon crawl <url>` | `axon <url> --scope site` |
| `axon code-search <query>` | `axon query <query> --content-kind code --freshness committed` |
| `axon code-search-watch` | `axon watch <path>` |
| `axon purge ...` | `axon prune ...` |
| `axon dedupe ...` | `axon prune dedupe ...` |
| `axon refresh ...` | `axon <source> --refresh` or source operation |
| `axon fresh ...` | `axon watch ...` or source freshness config |

The replacements are documentation guidance, not runtime aliases.

## Retained Scrape Command

`axon scrape <url>` remains public with these semantics:

- exactly one web page is fetched/rendered/normalized
- no sibling-link crawl, sitemap expansion, or discovered-link fanout occurs
- vectors are published by default (`embed=true`)
- `--no-embed` may opt out of vector publication
- clean normalized content is returned inline, written to an explicit path, or
  stored as an artifact according to `OutputPolicy`
- implementation goes through `SourceRequest`, not old scrape-specific request
  DTOs or handlers

Equivalent source-pipeline request:

```json
{
  "source": "https://example.com/page",
  "scope": "page",
  "embed": true
}
```

## Removed MCP Actions

| Removed | Canonical Replacement |
|---|---|
| `embed` | `source` |
| `ingest` | `source` |
| `scrape` | `source` with `scope=page` |
| `crawl` | `source` with `scope=site` |
| `code_search` | `query` with `content_kind=code`, source/path filters, and committed-generation freshness |
| `vertical_scrape` | adapter capabilities plus `source` |
| `purge` | `prune` |
| `dedupe` | `prune` |

Removed actions must not appear in the MCP schema.

The retained CLI `scrape` command does not require a retained MCP `scrape`
action. MCP callers use `action=source` with `scope=page` unless a future MCP
projection is explicitly added over the same `SourceRequest` contract.

## Removed REST Routes

| Removed | Canonical Replacement |
|---|---|
| `POST /v1/embed` | `POST /v1/sources` |
| `POST /v1/ingest` | `POST /v1/sources` |
| `POST /v1/scrape` | `POST /v1/sources` |
| `POST /v1/crawl` | `POST /v1/sources` |
| direct destructive deletion | `POST /v1/prune/plan` then `POST /v1/prune/exec` |
| `POST /v1/watch/{id}/run` | `POST /v1/watches/{watch_id}/exec` |

Removed routes must not appear in OpenAPI.

The retained CLI `scrape` command does not require a retained `/v1/scrape`
route. REST callers use `POST /v1/sources` with `scope=page`.

## Local Code Search Replacement

The old code-search surface is removed, but the behavior is not lost. Local
code indexing becomes normal source indexing, and local code retrieval becomes
normal query with code filters:

```text
axon /home/jmagar/workspace/axon --watch
axon query "where is provider cooling implemented" \
  --source /home/jmagar/workspace/axon \
  --content-kind code \
  --freshness committed
```

Required replacement semantics:

- source setup uses `SourceLedger` generations, manifest diffs, AST-backed
  parser facts, code chunking, `VectorStore` payloads, and cleanup debt
- query filters can target `source_id`, canonical URI, local path, repository,
  branch, content kind, language, symbol, and path prefix
- default code query freshness is `committed`; callers never see an
  uncommitted generation unless an explicit debug flag is used
- stale or in-progress refreshes surface warnings and job ids, not empty
  success-looking results
- local absolute paths are subject to execution-affinity and redaction policy
- there is no `code-search` or `code_search` compatibility dispatcher

## Removed Config Keys

Config keys can be deleted when:

- the desired `.env` / `config.toml` shape has a clear replacement, or
- the old key represented behavior that no longer exists.

Setup/doctor may report unknown keys and suggest editing the file manually.
There is no requirement to auto-migrate old config files.

Known removed/replaced keys:

| Removed Key | Replacement |
|---|---|
| `AXON_MCP_HTTP_HOST` | `AXON_HTTP_HOST` |
| `AXON_MCP_HTTP_PORT` | `AXON_HTTP_PORT` |
| `AXON_MCP_HTTP_TOKEN` | `AXON_HTTP_TOKEN` |
| `AXON_MCP_AUTH_MODE` | `AXON_AUTH_MODE` |
| `AXON_MCP_PUBLIC_URL` | `AXON_PUBLIC_URL` |
| `AXON_MCP_GOOGLE_CLIENT_ID` | `AXON_GOOGLE_CLIENT_ID` |
| `AXON_MCP_GOOGLE_CLIENT_SECRET` | `AXON_GOOGLE_CLIENT_SECRET` |
| `AXON_MCP_AUTH_ADMIN_EMAIL` | `AXON_AUTH_ADMIN_EMAIL` |
| `AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS` | `AXON_ALLOWED_REDIRECT_URIS` |
| `AXON_MCP_ALLOWED_ORIGINS` | `AXON_ALLOWED_ORIGINS` |
| `AXON_COLLECTION` | `server.default_collection` in `config.toml` |
| `AXON_HYBRID_CANDIDATES` | `retrieval.hybrid_candidates` in `config.toml` |
| `AXON_ASK_HYBRID_CANDIDATES` | `ask.hybrid_candidates` in `config.toml` |
| `AXON_INGEST_LANES` | `pipeline.ingest_lanes` in `config.toml` |
| `AXON_EMBED_DOC_TIMEOUT_SECS` | `providers.embedding.doc_timeout_secs` in `config.toml` |
| `AXON_WATCH_TICK_SECS` | `watch.tick_secs` in `config.toml` |
| `AXON_WATCH_LEASE_SECS` | `watch.lease_secs` in `config.toml` |

Known removed DTO/request fields:

| Removed Field | Replacement |
|---|---|
| `EmbedRequest.input` | `SourceRequest.source` |
| `EmbedRequest.source_type` | adapter-selected `SourceKind` / `SourceScope` |
| `IngestRequest.target` | `SourceRequest.source` |
| `IngestRequest.source_type` | adapter-selected `SourceKind` / `SourceScope` |
| `IngestRequest.include_source` | `SourceRequest.options.include_source` when an adapter supports it |
| `CrawlRequest.urls` | `SourceRequest.source` plus multi-source submission when supported |
| `ScrapeRequest.url` | `SourceRequest.source` with `scope=page` |
| `PurgeRequest.target` | `PruneSelector` |
| `PurgeRequest.prefix` | `PruneSelector` scope/options |
| `CodeSearchRequest.cwd` | `QueryRequest.filters.source_id` or local source filter |
| `CodeSearchRequest.path_prefix` | `QueryRequest.filters.path_prefix` |
| `CodeSearchRequest.no_freshness` | `QueryRequest.freshness` |

Removed fields must be absent from generated DTO schemas. They are not accepted
as hidden aliases.

## Test Requirements

Tests must prove:

- removed CLI commands are absent from help
- removed CLI commands do not dispatch
- `axon scrape <url>` is present in help and dispatches only through
  `SourceRequest { scope=page, embed=true }`
- removed MCP actions are absent from schema
- removed REST routes are absent from OpenAPI
- generated clients do not expose removed operations
- old code paths are not reachable from canonical commands/actions/routes
- canonical replacements perform the intended behavior
- local code query replacement preserves committed-generation safety, path
  filters, and progress visibility

## Developer Ergonomics

During an implementation branch, it is acceptable for a removed surface to fail
at compile time, fail parser construction, or be temporarily marked TODO while
the branch is in progress. The final branch state must delete the surface or
make it unreachable before side effects.
