# MCP Tools Reference -- Axon

## Design

Axon exposes exactly one MCP tool:

| Tool | Purpose | Primary parameter |
|------|---------|-------------------|
| `axon` | Unified action router for all crawl/scrape/embed/query/RAG operations | `action` |

This single-tool pattern routes all operations through `action` + optional `subaction`, keeping the MCP surface minimal while supporting 50+ operations.

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
| `response_mode` | enum | no | Response format: `path` (default), `inline`, `both`, `auto_inline` |

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
| `path` (default) | Write result to `.cache/axon-mcp/` artifact file, return metadata (path, bytes, sha256, preview) |
| `inline` | Return full result inline (capped/truncated) |
| `both` | Write artifact and return inline content |
| `auto_inline` | Inline if payload is below `AXON_INLINE_BYTES_THRESHOLD` (default 8192), otherwise artifact |

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
| `format` | enum | `markdown` | `markdown`, `html`, `rawHtml`, `json` |
| `embed` | bool | `true` | Auto-embed content into Qdrant |
| `root_selector` | string | -- | CSS selector for content root |
| `exclude_selector` | string | -- | CSS selector for elements to exclude |

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
| `collection` | string | server-configured (`AXON_COLLECTION`, default `cortex`) | Qdrant collection |
| `since` | string | -- | Filter: only docs after this date |
| `before` | string | -- | Filter: only docs before this date |
| `hybrid_search` | bool | -- | `false` forces dense-only; unset = server config (`AXON_HYBRID_SEARCH`, default true) |

### ask

RAG: semantic search + LLM answer synthesis with citations.

```json
{ "action": "ask", "query": "How does hybrid search work?" }
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `query` | string | -- | Question to answer |
| `graph` | bool | `false` | Inject Neo4j graph context (no-op in lite mode) |
| `diagnostics` | bool | `false` | Include retrieval diagnostics |
| `collection` | string | server-configured (`AXON_COLLECTION`, default `cortex`) | Qdrant collection |
| `since` | string | -- | Date filter (after) |
| `before` | string | -- | Date filter (before) |
| `hybrid_search` | bool | -- | `false` forces dense-only; unset = server config |

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

Fetch stored document chunks from Qdrant by URL.

```json
{ "action": "retrieve", "url": "https://docs.example.com/guide" }
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `url` | string | -- | URL to retrieve chunks for |
| `max_points` | usize | -- | Limit returned chunks |

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

### acp

Manage ACP adapter sessions (advanced). Subactions: `list_sessions`,
`fork_session`, `resume_session`, `set_model`, `ext_method`, `ext_notification`,
`logout`. See `crates/mcp/server/handlers_acp.rs` for the wire contract.

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
| `max_depth` | usize | 5 | Max crawl depth |
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

Source types: `github`, `reddit`, `youtube`, `sessions`.

### artifacts

MCP artifact file management:
```json
{ "action": "artifacts", "subaction": "list" }
```

Subactions: `head`, `grep`, `wc`, `read`, `list`, `delete`, `clean`, `search`.

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

- [ENV.md](ENV.md) -- MCP environment variables
- [PATTERNS.md](PATTERNS.md) -- dispatch and artifact patterns
- [../MCP-TOOL-SCHEMA.md](../MCP-TOOL-SCHEMA.md) -- wire contract (source of truth)
