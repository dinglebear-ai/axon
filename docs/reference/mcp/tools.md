# MCP Tools Reference -- Axon

## Design

Axon exposes exactly one MCP tool:

| Tool | Purpose | Primary parameter |
|------|---------|-------------------|
| `axon` | Unified action router for all crawl/scrape/embed/query/RAG operations | `action` |

This single-tool pattern routes all operations through `action` + optional `subaction`, keeping the MCP surface minimal while supporting every Axon action and its lifecycle subactions through one schema. The full, machine-generated wire contract lives in [`../MCP-TOOL-SCHEMA.md`](tool-schema.md); this page documents the common actions.

## Tool: `axon`

### Input schema

```json
{
  "action": "<action>",
  "subaction": "<subaction>",
  "url": "<url>",
  "urls": ["<url1>", "<url2>"],
  "query": "<search text>",
  "response_mode": "path|inline|both|auto_inline"
}
```

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `action` | enum | yes | Operation to perform |
| `subaction` | enum | for lifecycle families | Sub-operation (start, status, cancel, etc.) |
| `response_mode` | enum | no | Response format: `path`, `inline`, `both`, `auto_inline` (most actions default to `path`; `scrape` and `retrieve` default to inline paged reads) |

Additional parameters vary by action. See sections below.

### Response format

All responses use the canonical envelope:

```json
{
  "ok": true,
  "action": "scrape",
  "subaction": null,
  "data": { ... }
}
```

Error responses:

```json
{
  "ok": false,
  "error": "description of the failure"
}
```

### Response modes

| Mode | Behavior |
|------|----------|
| `path` | Write result to `.cache/axon-mcp/` artifact file, return metadata (path, bytes, sha256, preview) |
| `inline` | Return full result inline (capped/truncated) |
| `both` | Write artifact and return inline content |
| `auto_inline` | Inline if payload is below `AXON_INLINE_BYTES_THRESHOLD` (default 8192), otherwise artifact |

`scrape` and `retrieve` are the document-reading exceptions: when `response_mode` is omitted they return inline paged content first, with `next_cursor` for continuation.

## Direct actions

These actions do not require `subaction`.

### scrape

Scrape one or more URLs to markdown.

```json
{ "action": "scrape", "url": "https://example.com" }
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `url` | string | -- | URL to scrape |
| `render_mode` | enum | `auto_switch` | `http`, `chrome`, `auto_switch` |
| `format` | enum | `markdown` | `markdown`, `html`, `raw_html`, `json`, `llm` |
| `embed` | bool | `true` | Auto-embed content into Qdrant |
| `root_selector` | string | -- | CSS selector for content root |
| `exclude_selector` | string | -- | CSS selector for elements to exclude |
| `cursor` | string | -- | Opaque continuation cursor for the next document slice |
| `token_budget` | usize | `10000` | Approximate max tokens to return in this slice |

Returns inline page content with `content`, `truncated`, `token_estimate`, `next_cursor`, `remaining_tokens_estimate`, and `backend`.

### query

Semantic vector search against the Qdrant collection.

```json
{ "action": "query", "query": "embedding pipeline architecture" }
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `query` | string | -- | Search text |
| `limit` | usize | 10 | Max results |
| `offset` | usize | 0 | Skip N results |
| `collection` | string | server-configured (`[search].collection`, default `axon`; `AXON_COLLECTION` only as override) | Qdrant collection |
| `since` | string | -- | Filter: only docs after this date |
| `before` | string | -- | Filter: only docs before this date |
| `hybrid_search` | bool | -- | `false` forces dense-only; unset = server config (`[search].hybrid-enabled`, default true; `AXON_HYBRID_SEARCH` only as override) |

### ask

RAG: semantic search + LLM answer synthesis with citations.

```json
{ "action": "ask", "query": "How does hybrid search work?" }
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `query` | string | -- | Question to answer |
| `diagnostics` | bool | `false` | Include retrieval diagnostics |
| `explain` | bool | `false` | Return a per-candidate explain trace and skip LLM synthesis |
| `collection` | string | server-configured (`[search].collection`, default `axon`; `AXON_COLLECTION` only as override) | Qdrant collection |
| `since` | string | -- | Date filter (after) |
| `before` | string | -- | Date filter (before) |
| `hybrid_search` | bool | -- | `false` forces dense-only; unset = server config |

### summarize

Scrape one or more URLs and summarize the fetched markdown with the configured LLM backend.

```json
{ "action": "summarize", "url": "https://example.com" }
{ "action": "summarize", "urls": ["https://a.example", "https://b.example"] }
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `url` | string | -- | Single URL to scrape and summarize |
| `urls` | string[] | -- | Multiple URLs to scrape and summarize |
| `render_mode` | enum | server config | `http`, `chrome`, or `auto_switch` |
| `root_selector` | string | -- | Scope extraction before summarization |
| `exclude_selector` | string | -- | Remove elements before summarization |

### evaluate

Evaluate RAG quality against a baseline answer.

```json
{ "action": "evaluate", "query": "How good is retrieval for hybrid search?" }
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `query` | string | -- | Question to evaluate. `question` is accepted as an alias. |
| `diagnostics` | bool | `false` | Include retrieval diagnostics |
| `retrieval_ab` | bool | `false` | Compare hybrid RAG against dense-only RAG instead of RAG against baseline |
| `collection` | string | server-configured (`[search].collection`, default `axon`; `AXON_COLLECTION` only as override) | Qdrant collection |
| `since` | string | -- | Date filter (after) |
| `before` | string | -- | Date filter (before) |
| `hybrid_search` | bool | -- | `false` forces dense-only; unset = server config |

### suggest

Suggest new crawl targets from the current indexed source coverage.

```json
{ "action": "suggest", "focus": "refresh scheduler internals", "limit": 5 }
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `focus` | string | -- | Optional suggestion focus. `query` is accepted as an alias. |
| `limit` | usize | server search limit | Max suggestions to return |
| `collection` | string | server-configured (`[search].collection`, default `axon`; `AXON_COLLECTION` only as override) | Qdrant collection |

### search

Web search via Tavily, auto-queues crawl jobs for results.

```json
{ "action": "search", "query": "rust async patterns" }
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `query` | string | -- | Search query |
| `limit` | usize | 10 | Max results |
| `search_time_range` | enum | -- | `day`, `week`, `month`, `year` |

### research

Web research via Tavily AI search with LLM synthesis.

```json
{ "action": "research", "query": "vector database comparison 2025" }
```

Parameters are the same as `search`.

### retrieve

Read a known document URL from the best available backend: Qdrant chunks first, then stored source text, then live scrape refresh on miss or stale stored content.

```json
{ "action": "retrieve", "url": "https://docs.example.com/guide" }
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `url` | string | -- | URL to retrieve chunks for |
| `max_points` | usize | -- | Limit returned chunks |
| `cursor` | string | -- | Opaque continuation cursor for the next document slice |
| `token_budget` | usize | `10000` | Approximate max tokens to return in this slice |

Returns inline page content with `requested_url`, `matched_url`, `backend`, `warnings`, `variant_errors`, `refresh_status`, and the shared paging fields.

### map

Discover all URLs at a domain without scraping content.

```json
{ "action": "map", "url": "https://example.com" }
```

### screenshot

Capture a page screenshot via headless Chrome.

```json
{ "action": "screenshot", "url": "https://example.com" }
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `url` | string | -- | URL to capture |
| `full_page` | bool | `false` | Full page vs viewport |
| `viewport` | string | -- | Viewport dimensions |
| `output` | string | -- | Output file path |

### elicit_demo

Demo elicitation prompt — exercises the MCP elicitation request path end to end.

```json
{ "action": "elicit_demo", "message": "Pick a focus topic" }
```

### More direct actions

These additional direct actions are exposed by the same `axon` tool. See [`../MCP-TOOL-SCHEMA.md`](tool-schema.md) for their full field tables.

| Action | Required | Notable optional fields | Description |
|--------|----------|-------------------------|-------------|
| `brand` | `url` | `render_mode` | Extract brand identity (colors, fonts, logos, favicon). `render_mode` is accepted but currently ignored. |
| `diff` | `url_a`, `url_b` | `render_mode` | Compare two URLs (content/metadata/link changes). |
| `endpoints` | `url` | `include_bundles`, `first_party_only`, `unique_only`, `max_scripts`, `max_scan_bytes`, `verify`, `capture_network`, `probe_rpc`, `probe_rpc_subdomains` | Static endpoint/API discovery from page scripts. Read-scoped, side-effect-free; `capture_network` opt-in executes page code. |
| `debug` | -- | `context` | Run doctor plus LLM-assisted troubleshooting. |
| `dedupe` | -- | `collection` | Deduplicate near-identical chunks in a collection (admin scope). |
| `migrate` | -- | `from`, `to` | Copy an unnamed-vector collection into a new named-mode (dense + bm42) collection (admin scope). |
| `watch` | `subaction` | `id`, `name`, `task_type`, `task_payload`, `every_seconds`, `enabled`, `limit` | Recurring task scheduler. Subactions: `create`, `list`, `get`, `run_now`, `history` (`get` parses but is not yet implemented). |
| `setup` | -- | `mode` (`check`/`first-run`/`repair`/`migrate-env`) | First-run/local setup helper. |
| `vertical_scrape` | `subaction` | `extractor` | **Discovery only.** `list` returns the extractor catalog; `capabilities` returns per-extractor metadata. `run` is removed — use `scrape` instead (verticals fire automatically). |

## Lifecycle action families

These actions require a `subaction`. All support: `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover`.

### crawl

```json
{ "action": "crawl", "subaction": "start", "urls": ["https://example.com"] }
```

Start parameters:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `urls` | string[] | -- | Seed URLs (required) |
| `max_pages` | u32 | 0 (uncapped) | Page limit |
| `max_depth` | usize | 10 | Max crawl depth |
| `include_subdomains` | bool | `false` | Include subdomains |
| `respect_robots` | bool | `false` | Honor robots.txt |
| `discover_sitemaps` | bool | `true` | Run sitemap backfill |
| `render_mode` | enum | `auto_switch` | `http`, `chrome`, `auto_switch` |
| `delay_ms` | u64 | 0 | Per-request delay |

### extract

```json
{ "action": "extract", "subaction": "start", "urls": ["https://example.com/pricing"] }
```

### embed

```json
{ "action": "embed", "subaction": "start", "input": "path/to/file.md" }
```

### ingest

```json
{ "action": "ingest", "subaction": "start", "source_type": "github", "target": "owner/repo" }
```

Source types: `github`, `gitlab`, `gitea`, `git`, `reddit`, `youtube`, `sessions`.

### artifacts

MCP artifact file management:
```json
{ "action": "artifacts", "subaction": "list" }
```

Subactions: `head`, `grep`, `wc`, `read`, `list`, `delete`, `clean`, `search`.

Use `artifacts` for raw file/debug/admin access. Prefer `scrape`, `retrieve`, `query`, and `ask` for normal document consumption.

## Info actions

| Action | Description |
|--------|-------------|
| `doctor` | Check connectivity to all infrastructure services |
| `domains` | List all indexed domains with stats |
| `help` | Return full action reference as markdown |
| `sources` | List all indexed URLs with chunk counts |
| `stats` | Qdrant collection statistics |
| `status` | Async job queue status |

## Error semantics

| Error type | MCP code | Cause |
|------------|----------|-------|
| Input/shape failure | `invalid_params` | Missing required field, invalid enum value |
| Runtime failure | `internal_error` | Service unreachable, database error |

## See also

- [ENV.md](env.md) -- MCP environment variables
- [PATTERNS.md](patterns.md) -- dispatch and artifact patterns
- [../MCP-TOOL-SCHEMA.md](tool-schema.md) -- wire contract (source of truth)
