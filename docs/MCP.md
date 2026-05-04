# Axon MCP Server Guide
Last Modified: 2026-03-11

## Purpose
`axon mcp` exposes Axon through one MCP tool named `axon`.

- Transport: RMCP `stdio`, streamable HTTP (`/mcp`), or both simultaneously
- Tool count: 1
- Tool name: `axon`
- Routing fields: `action` + `subaction` for lifecycle families
- Response behavior field: `response_mode` (`path|inline|both`, default `path`; `auto-inline` is a server-emitted value indicating the auto-inline path was taken — it cannot be requested by the caller)
- Resources: `axon://schema/mcp-tool`, `ui://axon/status-dashboard`
- MCP Apps capability is enabled so compatible hosts can render the status dashboard widget

Canonical schema and action contract:
- `docs/MCP-TOOL-SCHEMA.md`

Implementation:
- `crates/mcp/schema.rs`
- `crates/mcp/server.rs`
- `crates/mcp/config.rs`

## Runtime Model
`axon mcp` is expected to run in the same environment as Axon workers.

Core stack env vars are reused:
- `QDRANT_URL`
- `TEI_URL`
- `OPENAI_BASE_URL`
- `OPENAI_API_KEY`
- `OPENAI_MODEL`
- `TAVILY_API_KEY`

MCP HTTP env vars:
- `AXON_MCP_HTTP_HOST` (default `127.0.0.1`)
- `AXON_MCP_HTTP_PORT` (default `8001`)
- `AXON_MCP_HTTP_TOKEN` (required for non-loopback binds; enforced on all MCP HTTP requests when set)

## Authentication

HTTP transport enforces `AXON_MCP_HTTP_TOKEN` when configured. Tokenless HTTP is
allowed only on loopback binds (`127.0.0.1`, `::1`, or `localhost`). Binding the
MCP HTTP server to a non-loopback address such as `0.0.0.0` requires
`AXON_MCP_HTTP_TOKEN`; otherwise startup is rejected. External OAuth gateways or
reverse proxies may add additional ingress controls.

Clients can authenticate with either header:

```bash
curl -H "Authorization: Bearer $AXON_MCP_HTTP_TOKEN" \
  http://127.0.0.1:8001/mcp
curl -H "x-api-key: $AXON_MCP_HTTP_TOKEN" \
  http://127.0.0.1:8001/mcp
```

## Transport Notes
`axon mcp` supports three transport modes:

- `axon mcp`
  Starts stdio transport only. Use this for local MCP clients such as Claude Desktop.
- `axon serve mcp`
  Starts HTTP transport only.
- `axon mcp --transport both`
  Starts stdio and HTTP concurrently.

HTTP transport uses:
- `AXON_MCP_HTTP_HOST` (default `127.0.0.1`)
- `AXON_MCP_HTTP_PORT` (default `8001`)
- `AXON_MCP_HTTP_TOKEN` (required for non-loopback binds)

## ACP MCP Server Store (Web UI + Pulse ACP)

ACP sessions (`pulse_chat`) read MCP server definitions from:

- `${AXON_DATA_DIR}/axon/mcp.json` when `AXON_DATA_DIR` is set
- `~/.config/axon/mcp.json` fallback when `AXON_DATA_DIR` is unset

The Web UI MCP settings page (`/api/mcp`) writes to this same file, so servers
added in the UI are the servers passed into ACP sessions.

Hot reload behavior:
- ACP watches `mcp.json` changes via file metadata checks.
- When MCP server config changes, Pulse ACP respawns the persistent adapter
  session with the updated MCP server list on the next turn.

### Config File Examples

Stdio MCP server example:

```json
{
  "mcpServers": {
    "axon-stdio": {
      "command": "axon",
      "args": ["mcp", "--transport", "stdio"]
    }
  }
}
```

HTTP MCP server example:

```json
{
  "mcpServers": {
    "axon-http": {
      "url": "https://axon.example.com/mcp"
    }
  }
}
```

### Claude Desktop Example

```json
{
  "mcpServers": {
    "axon": {
      "command": "axon",
      "args": ["mcp", "--transport", "stdio"],
      "env": {
        "QDRANT_URL": "http://127.0.0.1:53333",
        "TEI_URL": "http://YOUR_TEI_HOST:52000"
      }
    }
  }
}
```

## Request Pattern
Primary pattern:

```json
{
  "action": "<operation>",
  "...": "operation fields"
}
```

Lifecycle pattern when needed:

```json
{
  "action": "ingest|extract|embed|crawl",
  "subaction": "<action-specific subaction>",
  "...": "subaction fields"
}
```

## Preferred Action Names (Top-Level)
Use CLI-identical action names:
- `ingest`, `extract`, `embed`, `crawl`
- `query`, `retrieve`
- `doctor`, `domains`, `sources`, `stats`
- `search`, `map`
- `artifacts` (with subactions `head|grep|wc|read|list|delete|clean|search`)
- `scrape`, `research`, `ask`, `screenshot`, `help`, `status`, `elicit_demo`

Examples:
- `action: "ingest", subaction: "start"`
- `action: "extract", subaction: "list"`
- `action: "query"`
- `action: "doctor"`

## Parser Rules
The server uses strict deserialization:
- `action` is required and must match canonical schema names exactly
- `subaction` is required for lifecycle families (`crawl|extract|embed|ingest|artifacts`)
- No fallback fields (`command|op|operation`)
- No action alias remapping
- No token normalization (`-`/spaces/case are not rewritten)

## Online Operations
Direct actions:
- `help`
- `scrape`
- `research`
- `ask`
- `screenshot`
- `elicit_demo`

Lifecycle families:
- `crawl`: `start|status|cancel|list|cleanup|clear|recover`
- `extract`: `start|status|cancel|list|cleanup|clear|recover`
- `embed`: `start|status|cancel|list|cleanup|clear|recover`
- `ingest`: `start|status|cancel|list|cleanup|clear|recover`

No top-level aliases are supported.

## Response Pattern
Success responses are normalized:

```json
{
  "ok": true,
  "action": "...",
  "subaction": "...",
  "data": { "...": "..." }
}
```

## mcporter Smoke Tests
```bash
# Primary MCP smoke path.
bash ./scripts/test-mcp-tools-mcporter.sh

# Local introspection against the repo's mcporter config
mcporter --config config/mcporter.json list axon --schema
mcporter --config config/mcporter.json call axon.axon action:help response_mode:inline --output json
mcporter --config config/mcporter.json call axon.axon action:doctor --output json
mcporter --config config/mcporter.json call axon.axon action:scrape url:https://www.rust-lang.org/learn/get-started --output json
mcporter --config config/mcporter.json call axon.axon action:query query:'rust mcp sdk' --output json
mcporter --config config/mcporter.json call axon.axon action:crawl subaction:list limit:5 offset:0 --output json
mcporter --config config/mcporter.json call axon.axon action:artifacts subaction:list --output json
```

What the smoke harness enforces:
- `mcporter list --schema` exposes the `axon` tool and the expected top-level actions.
- `action:help` exposes the full routed surface, including all lifecycle subactions.
- Every exposed route has a real smoke case.
- Suite logs and generated configs live under `.cache/mcporter-test/`.

## Artifact Inspection Workflow

Artifact responses written in path mode are pretty-printed JSON. The preferred inspection order (least to most expensive):

1. **Shape summary** — path-mode responses include a `shape` field that summarises key/value types without reading the file. Often sufficient.
2. `artifacts head` — first N lines (default 25). Quick orientation for any artifact.
3. `artifacts grep pattern="..." context_lines=N` — regex search with context. Targeted lookup.
4. `artifacts search pattern="..."` — cross-artifact regex search. Find which files contain a term.
5. `artifacts read pattern="..."` — filtered line dump. Reads whole file but returns only matching lines.
6. `artifacts read full=true` — full paginated dump. Last resort; explicit opt-in required.

### Artifact Lifecycle

- `artifacts list` — all files in artifact dir, sorted newest first (name, bytes, age).
- `artifacts delete path=<path>` — delete a single file. Path is validated to be within artifact root.
- `artifacts clean max_age_hours=N` — bulk cleanup. `dry_run` defaults to `true` (preview only).
  - `max_age_hours` is **required** — there is no default. Caller must declare intent explicitly.
  - Never deletes files inside `screenshots/` — those are user assets, not ephemeral artifacts.
  - Set `dry_run=false` to execute the deletion after reviewing the preview.

### `response_mode` on All Actions

All actions support `response_mode`. Default is `path`, writing the payload to an artifact and returning a compact shape summary. Use `response_mode=inline` to get the payload directly in the response.

Valid `response_mode` values: `path|inline|both|auto-inline`. Note that `auto-inline` is system-assigned — it cannot be requested by the caller. See [`MCP-TOOL-SCHEMA.md`](MCP-TOOL-SCHEMA.md) for the full enum definition.

**Per-action response overrides (InlineHint):** Some actions override the standard response behavior regardless of `response_mode`:

- **`ask`** and **`research`**: Always write the full payload to an artifact AND include `key_fields.answer` / `key_fields.summary` directly in the path-mode response. This means the LLM answer is always immediately readable without an `artifacts.head` follow-up, regardless of its length.
- **`scrape`** and **`retrieve`**: Always return path mode regardless of the requested `response_mode`. These payloads can be megabytes of content — use `artifacts.head` or `artifacts.grep` with `relative_path` to access the content.

### Unified Artifact Access Model

The artifact cache is server-centric. All clients — local stdio and remote HTTP — should use `artifacts.*` subactions with the `relative_path` field to access artifact content:

```bash
# Access content via relative_path (works for all clients)
mcporter call axon.axon action:artifacts subaction:head path:"ask/what-is-hybrid-search.json"
```

The `path` field (absolute filesystem path) is present in artifact metadata for transparency and debugging only. Do not depend on it — remote clients cannot open server-side paths directly.

### Auto-inline for Small Payloads

Regardless of the requested `response_mode`, any payload serializing to ≤ `AXON_INLINE_BYTES_THRESHOLD` bytes (default 8 192) is returned inline without requiring an `artifacts.read` follow-up call. The response includes `"response_mode": "auto-inline"`, the full `data` object, and an `artifact` pointer for persistence. Set `AXON_INLINE_BYTES_THRESHOLD=0` to disable auto-inline and always use explicit `response_mode` selection.

### Shape Preview Improvements

Path-mode responses include a `shape` field summarizing the payload structure:
- **Strings ≤ 100 chars**: returned verbatim so Claude reads real values without a follow-up read.
- **Strings > 100 chars**: summarized as `"<string N>"`.
- **Arrays of objects with a `status`, `phase`, or `state` field**: summarized as `{"total": N, "by_status": {"completed": N, "running": N, ...}}`. Claude can answer status questions from the shape alone — no follow-up read needed.
- **Other arrays**: `{"total": N, "sample": [<first 2 items, shape-previewed>]}`. The sample items let you understand the data structure without reading the file.
- **Primitives**: verbatim.
