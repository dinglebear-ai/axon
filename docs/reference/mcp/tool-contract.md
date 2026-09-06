# MCP Tool Contract

Last Modified: 2026-07-19

Axon exposes MCP through **one** operation tool named `axon`. Callers send an
`action` (and optional `subaction`) plus a typed request body; the server maps
the request to `axon-api` DTOs, calls `axon-services`, and returns a typed
result.

> Live source of truth: [`tool-schema.md`](tool-schema.md) (160 lines) and
> [`tool-schema.json`](tool-schema.json). Contract source:
> [`docs/pipeline-unification/surfaces/tool-contract.md`](../../pipeline-unification/surfaces/tool-contract.md).
> Implementation: `crates/axon-mcp/src/schema.rs`, `crates/axon-mcp/src/server.rs`.

## Canonical envelope

```json
{ "ok": true, "action": "<resolved action>", "subaction": "<resolved subaction>", "data": { "..." : "..." } }
```

Parser is strict serde: `action` is required and must match a canonical name;
`subaction` is optional for lifecycle families (defaults to `start` when
omitted for `extract`). There are no fallback fields (`command`/`op`/`operation`),
no token normalization, and no alias remapping.

## Direct actions (no subaction required)

| Action | Required field |
|---|---|
| `ask` | `query` |
| `query` | `query` |
| `research` | `query` |
| `evaluate` | `query` |
| `brand` | `url` |
| `endpoints` | `url` |
| `map` | `url` |
| `screenshot` | `url` |
| `diff` | `url_a`, `url_b` |
| `retrieve` | `url` |
| `summarize` | `url` or `urls` |
| `source` | none (optional `source`/`scope`/`collection`/`response_mode`/`detached`) |
| `doctor`, `help`, `prune`, `status`, `suggest` | none |

## Lifecycle action families

| Family | Subactions |
|---|---|
| `extract` | `start` (requires `urls` array) |
| `memory` | `remember` / `list` / `search` / `show` / `link` / `supersede` / `context` / `reinforce` / `contradict` / `pin` / `archive` / `forget` / `review` / `compact` / `import` / `export` |

## Source acquisition

Universal indexing goes through `action=source` with
`scope=page`/`site`/`docs`/`repo`/`package`/`subreddit`. Focused `scrape`,
`crawl`, `embed`, and `ingest` actions project onto the same `SourceRequest`
pipeline; `code_search` is a committed-state read projection.

## Response modes

`response_mode` (`ResponseMode` enum):

| Mode | Behavior |
|---|---|
| `path` | Return artifact-ref metadata; client follows `artifact_id` |
| `inline` | Inline the full payload |
| `both` | Inline + artifact path |
| `auto_inline` (default) | Inline small payloads; artifact path for large ones |

MCP responses never expose a server filesystem path; clients follow the
returned `artifact_id`. Heavy operations write artifacts under
`~/.axon/artifacts/<context>/` (override with `AXON_MCP_ARTIFACT_DIR`).
Compact metadata fields: `path`, `bytes`, `line_count`, `sha256`, `preview`,
`preview_truncated`.

## Task support

The server advertises the SEP-2663 `io.modelcontextprotocol/tasks` extension in
its `extensions` capability map. Task support is **server-wide, not per-tool** —
rmcp 3.x removed the `execution.taskSupport` tool field, so no tool advertises
one. A client opts an individual `tools/call` into task mode by carrying the
`io.modelcontextprotocol/tasks` key in the request `_meta` and declaring the
tasks extension capability itself; without that key the call is served
synchronously. **Task starts are supported for `extract.start` only.** Task IDs
are stable aliases over Axon job IDs: `axon:<kind>:<job_uuid>`. The lifecycle
surface is `tasks/get` and `tasks/cancel` — SEP-2663 removed `tasks/result` and
`tasks/list`, and `tasks/get` now inlines the terminal result (or error) in the
returned `DetailedTask`. Poll interval ≥ 5000 ms.

## Resources

- `axon://schema/mcp-tool` — this tool's schema.
- `ui://axon/status-dashboard` — MCP Apps status-dashboard widget (presentation
  only; not an operation surface).

## Transport and auth

| Command | Transport |
|---|---|
| `axon mcp` | stdio (default) |
| `axon mcp --transport http` | streamable HTTP at `/mcp` |
| `axon mcp --transport both` | stdio + HTTP concurrently |
| `axon serve mcp` | unified web + MCP HTTP on one listener |

MCP HTTP auth shares the same policy as the unified HTTP server: loopback
allows tokenless; non-loopback requires `AXON_HTTP_TOKEN` or OAuth
(`AXON_AUTH_MODE=oauth`). See [overview.md](overview.md).

## Error semantics

- Input/shape failures → MCP `invalid_params`.
- Runtime failures → MCP `internal_error`.

## Removed actions

Removed legacy actions (`vertical_scrape`, `purge`, `dedupe`, and
`code_search_watch`) are **not** valid `action` enum values and are rejected
before dispatch. Use `source` or a focused source projection for indexing and
`prune` for cleanup.

If the MCP action router or response envelope changes, update `crates/axon-mcp/src/schema.rs` and
re-run `python3 scripts/generate_mcp_schema_doc.py` in the same PR.
