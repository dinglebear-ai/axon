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
- `AXON_PG_URL`
- `AXON_REDIS_URL`
- `AXON_AMQP_URL`
- `QDRANT_URL`
- `TEI_URL`
- `OPENAI_BASE_URL`
- `OPENAI_API_KEY`
- `OPENAI_MODEL`
- `TAVILY_API_KEY`

MCP transport env vars:
- `AXON_MCP_TRANSPORT` (`http` default; `stdio|http|both`)
- `AXON_MCP_HTTP_HOST` (default `0.0.0.0`)
- `AXON_MCP_HTTP_PORT` (default `8001`)

MCP HTTP auth env vars:
- `AXON_MCP_API_KEY` (optional static bearer token for `/mcp`)

OAuth broker env vars (optional; enables OAuth bearer tokens for `/mcp`):
- `GOOGLE_OAUTH_CLIENT_ID`
- `GOOGLE_OAUTH_CLIENT_SECRET`

Optional OAuth overrides:
- `GOOGLE_OAUTH_AUTH_URL`
- `GOOGLE_OAUTH_TOKEN_URL`
- `GOOGLE_OAUTH_REDIRECT_PATH`
- `GOOGLE_OAUTH_REDIRECT_HOST`
- `GOOGLE_OAUTH_REDIRECT_URI`
- `GOOGLE_OAUTH_BROKER_ISSUER`
- `GOOGLE_OAUTH_SCOPES`
- `GOOGLE_OAUTH_DCR_TOKEN`
- `GOOGLE_OAUTH_REDIRECT_POLICY`
- `GOOGLE_OAUTH_REDIS_URL` (falls back to `AXON_REDIS_URL`)
- `GOOGLE_OAUTH_REDIS_PREFIX`

`GOOGLE_OAUTH_REDIRECT_POLICY` modes:
- `loopback_or_https` (default): allow loopback HTTP callbacks (`localhost`, `127.0.0.1`, `::1`) and any HTTPS callback
- `loopback_only`: allow only loopback HTTP callbacks
- `any`: allow any HTTP/HTTPS callback URI

`/mcp` accepts either `Authorization: Bearer <AXON_MCP_API_KEY>` or a valid OAuth bearer token (`atk_...`). If neither auth mode is configured, requests to `/mcp` return unauthorized.

## Transport Notes
`axon mcp` supports three transport modes:

- `axon mcp`
  Starts HTTP transport only. This remains the default for backward compatibility.
- `axon mcp --transport stdio`
  Starts stdio transport only. Use this for local MCP clients such as Claude Desktop.
- `axon mcp --transport both`
  Starts stdio and HTTP concurrently.

Equivalent env override:

```bash
AXON_MCP_TRANSPORT=stdio   # or http / both
```

HTTP transport uses:
- `AXON_MCP_HTTP_HOST` (default `0.0.0.0`)
- `AXON_MCP_HTTP_PORT` (default `8001`)

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
      "url": "https://axon.example.com/mcp",
      "headers": {
        "Authorization": "Bearer <AXON_MCP_API_KEY>"
      }
    }
  }
}
```

OAuth bearer token is also accepted on `/mcp`:

```json
{
  "mcpServers": {
    "axon-http": {
      "url": "https://axon.example.com/mcp",
      "headers": {
        "Authorization": "Bearer atk_your_oauth_token_here"
      }
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
        "AXON_PG_URL": "postgresql://axon:postgres@127.0.0.1:53432/axon",
        "AXON_REDIS_URL": "redis://127.0.0.1:53379",
        "AXON_AMQP_URL": "amqp://axon:axonrabbit@127.0.0.1:45535/%2f",
        "QDRANT_URL": "http://127.0.0.1:53333",
        "TEI_URL": "http://YOUR_TEI_HOST:52000"
      }
    }
  }
}
```

## OAuth Endpoints and Flow
Implemented endpoints:
- `GET /oauth/google/status`
- `GET /oauth/google/login`
- `GET /oauth/google/callback`
- `GET /oauth/google/token`
- `GET|POST /oauth/google/logout`
- `GET /.well-known/oauth-protected-resource`
- `GET /.well-known/oauth-authorization-server`
- `POST /oauth/register`
- `GET /oauth/authorize`
- `POST /oauth/token`

High-level flow:
1. Client discovers metadata from the `/.well-known/*` endpoints.
2. Client registers (`/oauth/register`) if needed.
3. User authenticates via Google (`/oauth/google/login` -> Google -> `/oauth/google/callback`).
4. Authorization code flow completes via `/oauth/authorize` and `/oauth/token`.
5. Client calls `/mcp` with bearer token.

Static API key alternative:
- Clients can call `/mcp` directly with `Authorization: Bearer <AXON_MCP_API_KEY>`.
- API key auth is full `/mcp` access and does not apply OAuth scope checks.
- OAuth discovery endpoints remain available unchanged.

## Token Persistence
OAuth state is persisted in Redis when available; otherwise in-memory fallback is used.

Stored record types:
- pending login state
- browser session tokens
- dynamic clients
- auth codes
- access tokens
- refresh tokens
- rate-limit buckets

Cookie:
- `__Host-axon_oauth_session`

TTL semantics (current behavior):
- OAuth session: 7 days
- Refresh tokens: 30 days
- Auth code: 10 minutes
- Pending login state: 15 minutes
- Access token: per-issued token expiry

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
  "action": "ingest|extract|embed|crawl|refresh|graph",
  "subaction": "<action-specific subaction>",
  "...": "subaction fields"
}
```

## Preferred Action Names (Top-Level)
Use CLI-identical action names:
- `ingest`, `extract`, `embed`, `crawl`, `refresh`
- `graph` (with subactions `build|status|explore|stats`)
- `query`, `retrieve`
- `doctor`, `domains`, `sources`, `stats`
- `search`, `map`
- `artifacts` (with subactions `head|grep|wc|read|list|delete|clean|search`)
- `scrape`, `research`, `ask`, `screenshot`, `export`, `help`, `status`, `elicit_demo`

Examples:
- `action: "ingest", subaction: "start"`
- `action: "extract", subaction: "list"`
- `action: "query"`
- `action: "doctor"`

## Parser Rules
The server uses strict deserialization:
- `action` is required and must match canonical schema names exactly
- `subaction` is required for lifecycle families (`crawl|extract|embed|ingest|refresh|graph|artifacts`)
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
- `export`
- `elicit_demo`

Lifecycle families:
- `crawl`: `start|status|cancel|list|cleanup|clear|recover`
- `extract`: `start|status|cancel|list|cleanup|clear|recover`
- `embed`: `start|status|cancel|list|cleanup|clear|recover`
- `ingest`: `start|status|cancel|list|cleanup|clear|recover`
- `refresh`: `start|status|cancel|list|cleanup|clear|recover|schedule`
- `graph`: `build|status|explore|stats`

Refresh schedule subactions:
- `list`
- `create`
- `delete`
- `enable`
- `disable`

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
# Runs both suites:
# - full mode: AXON_LITE=0
# - lite mode: AXON_LITE=1
bash ./scripts/test-mcp-tools-mcporter.sh

# Local introspection against the repo's mcporter config
mcporter --config config/mcporter.json list axon --schema
mcporter --config config/mcporter.json call axon.axon action:help response_mode:inline --output json
mcporter --config config/mcporter.json call axon.axon action:doctor --output json
mcporter --config config/mcporter.json call axon.axon action:scrape url:https://www.rust-lang.org/learn/get-started --output json
mcporter --config config/mcporter.json call axon.axon action:query query:'rust mcp sdk' --output json
mcporter --config config/mcporter.json call axon.axon action:crawl subaction:list limit:5 offset:0 --output json
mcporter --config config/mcporter.json call axon.axon action:refresh subaction:list limit:5 offset:0 --output json
mcporter --config config/mcporter.json call axon.axon action:refresh subaction:schedule schedule_subaction:list --output json
mcporter --config config/mcporter.json call axon.axon action:artifacts subaction:list --output json
```

What the smoke harness enforces:
- `mcporter list --schema` exposes the `axon` tool and the expected top-level actions.
- `action:help` exposes the full routed surface, including nested lifecycle and `refresh schedule` subactions.
- Every exposed route has a real smoke case.
- Full mode requires successful responses for the full MCP surface.
- Lite mode requires successful responses for lite-supported routes and intentional unavailability for `export` and `graph:*`.
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

`doctor`, `stats`, and `status` now support `response_mode`. Default is `path`, writing the payload to an artifact and returning a compact shape summary. Use `response_mode=inline` to get the payload directly in the response.

Valid `response_mode` values: `path|inline|both|auto-inline`. Note that `auto-inline` is system-assigned — it cannot be requested by the caller. See [`MCP-TOOL-SCHEMA.md`](MCP-TOOL-SCHEMA.md) for the full enum definition.

### Auto-inline for Small Payloads

Regardless of the requested `response_mode`, any payload serializing to ≤ `AXON_INLINE_BYTES_THRESHOLD` bytes (default 8 192) is returned inline without requiring an `artifacts.read` follow-up call. The response includes `"response_mode": "auto-inline"`, the full `data` object, and an `artifact` pointer for persistence. Set `AXON_INLINE_BYTES_THRESHOLD=0` to disable auto-inline and always use explicit `response_mode` selection.

### Shape Preview Improvements

Path-mode responses include a `shape` field summarizing the payload structure:
- **Strings ≤ 100 chars**: returned verbatim so Claude reads real values without a follow-up read.
- **Strings > 100 chars**: summarized as `"<string N>"`.
- **Arrays of objects with a `status`, `phase`, or `state` field**: summarized as `{"total": N, "by_status": {"completed": N, "running": N, ...}}`. Claude can answer status questions from the shape alone — no follow-up read needed.
- **Other arrays**: `"<array[N]>"`.
- **Primitives**: verbatim.
