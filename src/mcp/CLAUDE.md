# crates/mcp — Axon MCP Server Guide
Last Modified: 2026-03-28

## Purpose
`crates/mcp` implements the Axon Model Context Protocol server (`axon mcp`) that exposes crawler/RAG capabilities through a single MCP tool.

- CLI entrypoint: `main.rs` subcommand `mcp`
- Transport: RMCP streamable HTTP on `/mcp` (CLI entrypoint uses HTTP)
- MCP tool: `axon`
- Routing model: consolidated `action` + `subaction`

## Module Layout

```
crates/mcp.rs           # Crate-root re-export shim (sibling to crates/mcp/)
mcp/
├── auth.rs                     # OAuth token storage + auth helpers
├── cors.rs                     # CORS middleware for the HTTP transport
├── schema.rs                   # AxonRequest / AxonToolResponse types, action/subaction enums
├── schema/
│   └── tests.rs                # Schema parser + dispatch tests
├── server.rs                   # AxonMcpServer: tool registration + action dispatch router
├── server/
│   ├── common.rs                   # Shared handler utilities
│   ├── http.rs                     # HTTP transport plumbing
│   ├── handlers_crawl_extract.rs   # crawl + extract action handlers
│   ├── handlers_embed_ingest.rs    # embed + ingest action handlers
│   ├── handlers_query.rs           # query, retrieve, search, map, scrape, ask, research
│   ├── handlers_elicit.rs          # elicitation prompts
│   ├── handlers_system.rs          # doctor, domains, sources, stats, status, artifacts, help
│   ├── handlers_system/
│   │   └── screenshot.rs           # screenshot handler split off from handlers_system
│   ├── artifacts.rs                # Artifact response wrapper
│   ├── artifacts/
│   │   ├── lifecycle.rs            # Artifact lifecycle (creation, eviction)
│   │   ├── path.rs                 # artifact path helpers — default `$AXON_DATA_DIR/artifacts/<context>` (`~/.axon/artifacts/<context>`)
│   │   ├── respond.rs              # Inline-vs-path response shaping
│   │   └── shape.rs                # Artifact response shape
│   └── services_migration_tests.rs # Migration tests for the services-layer plumbing
└── assets/
    └── status_dashboard.html       # MCP App resource for `ui://axon/status-dashboard`
```

There is no `crates/mcp/config.rs`. The `load_mcp_config()` helper that used to live here was removed when the MCP server adopted the unified `build_config()` path; OAuth token storage helpers live in `auth.rs`.

## Source-of-Truth References
- Wire contract schema doc: `docs/MCP-TOOL-SCHEMA.md`
- MCP runtime/design doc: `docs/MCP.md`
- Tool request/response types: `crates/mcp/schema.rs`
- Tool router and dispatch: `crates/mcp/server.rs`
- Handler implementations: `crates/mcp/server/handlers_*.rs`
- Runtime config: threaded in via `build_config()` from `crates/core/config/parse/build_config.rs` (no dedicated MCP config loader)

If documentation and code diverge, update both in the same change.

## Consolidated Tool Pattern
The single `axon` tool is the only public MCP tool. All operations route through:

```json
{
  "action": "<domain>",
  "subaction": "<operation>",
  "...": "operation params"
}
```

Domains (`action`):
- `help`
- `status`
- `crawl`
- `extract`
- `embed`
- `ingest`
- `query`
- `retrieve`
- `search`
- `map`
- `scrape`
- `ask`
- `research`
- `screenshot`
- `doctor`
- `domains`
- `sources`
- `stats`
- `artifacts`

This pattern is mandatory. Do not add separate MCP tools for each operation.

## Current Action Map

### `crawl`
- `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover`
- Integration: `crates/jobs/crawl.rs`

### `extract`
- `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover`
- Integration: `crates/jobs/extract.rs`

### `embed`
- `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover`
- Integration: `crates/jobs/embed.rs`

### `ingest`
- `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover`
- Integration: `crates/jobs/ingest.rs`

### `ask`
- Direct action (no subaction)
- Integration: `crates/vector/ops/commands/ask/`

### `research`
- Direct action (no subaction)
- Integration: `spider_agent` Tavily AI search + LLM synthesis

### `screenshot`
- Direct action (no subaction)
- Integration: `crates/cli/commands/screenshot/`

### `status`
- Direct action (no subaction)
- Integration: job queue status across all job types

### `query` / `retrieve`
- Integration: `crates/vector/ops/tei.rs`, `crates/vector/ops/qdrant/*`

### `search` / `map` / `scrape`
- Integration: `crates/core/http.rs`, `crates/core/content.rs`, `crates/crawl/engine.rs`, `spider_agent`

### `doctor` / `domains` / `sources` / `stats`
- Integration: lightweight probes + qdrant endpoints

### `artifacts`
- `head`, `grep`, `wc`, `read`
- Integration: artifact files in `$AXON_MCP_ARTIFACT_DIR` or `$AXON_DATA_DIR/artifacts/<context>` (default: `~/.axon/artifacts/<context>`)

### `help`
- `run` (implicit direct action)
- Returns all actions/subactions/resources

## Error Contract
Use MCP-native errors:
- invalid request/params -> `ErrorData::invalid_params(...)`
- runtime/system failure -> `ErrorData::internal_error(...)`

Rules:
- Validate required fields early.
- Return deterministic error messages (action/subaction context).
- Never leak secrets in errors.

## Response Contract
Success responses are normalized by `AxonToolResponse`:

```json
{
  "ok": true,
  "action": "...",
  "subaction": "...",
  "data": { ... }
}
```

Keep payloads stable and additive. Avoid breaking field renames.

Default response behavior is artifact-first:
- `response_mode` defaults to `path`
- Large outputs persist under `$AXON_MCP_ARTIFACT_DIR` (default: `~/.axon/artifacts/<context>`)
- Inline responses are capped and include artifact pointers

## Configuration Model

MCP config is threaded in directly via `build_config()` from `crates/core/config/parse/build_config.rs`. There is no `load_mcp_config()` function — it was removed when config was unified (commit `54244286`). The MCP server reads the standard `Config` struct like every other command.

Expected runtime model:
- `axon mcp` runs inside the same stack environment as workers.
- Existing `.env`/container env is sufficient.

## `ServiceContext` Wiring

All MCP action handlers receive a `&ServiceContext` (from `crates/services/context.rs`) constructed once at server startup:

```rust
// In handler dispatch
let ctx = ServiceContext::new_with_workers(Arc::new(cfg)).await?;
// ...
self.handle_crawl(request).await
```

`ServiceContext` carries exactly two fields — `cfg: Arc<Config>` and `jobs: Arc<dyn ServiceJobRuntime>`. **There is no `capabilities` field and no `ServiceCapabilities` struct.** Any documentation referring to `ctx.capabilities.<cap>.supported` is stale; gating now lives inside the individual service functions (e.g. the watch scheduler returns its own "not supported" error). The legacy `graph`, `refresh`, and `export` MCP actions were removed entirely when full mode was retired.

## Implementation Rules
1. Keep one tool (`axon`) only.
2. Add new capability by extending `action/subaction`, not adding new tool names.
3. Update all three layers together:
   - `schema.rs`
   - `server.rs`
   - `docs/MCP-TOOL-SCHEMA.md`
4. If behavior changes materially, also update `docs/MCP.md` and root docs references.
5. Prefer direct calls into existing Axon job/vector APIs over shelling out.

## Testing Workflow
Build:

```bash
cargo run --bin axon -- mcp
```

Primary MCP smoke path:

```bash
bash ./scripts/test-mcp-tools-mcporter.sh
```

Schema/introspection:

```bash
mcporter --config config/mcporter.json list axon --schema
```

Smoke calls:

```bash
mcporter --config config/mcporter.json call axon.axon action:doctor --output json
mcporter --config config/mcporter.json call axon.axon action:sources limit:5 --output json
mcporter --config config/mcporter.json call axon.axon action:crawl subaction:list limit:5 --output json
```

Lite mode is the only mode. The smoke harness runs against `AXON_LITE=1`; when adding a new action or subaction, add at least one smoke case.

## Change Checklist (Mandatory)
- [ ] `schema.rs` updated
- [ ] `server.rs` routing/handler updated
- [ ] docs contract updated (`docs/MCP-TOOL-SCHEMA.md`)
- [ ] `cargo check --bin axon` still passes

This is the way.
