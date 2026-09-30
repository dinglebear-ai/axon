# MCP Development Workflow -- Axon

Day-to-day development guide for the Axon MCP server.

## Quick Start

```bash
git clone https://github.com/dinglebear-ai/axon.git
cd axon
mkdir -m 700 -p ~/.axon
test -e ~/.axon/.env || cp .env.example ~/.axon/.env
# Edit ~/.axon/.env with service URLs/secrets.

just dev
```

`just dev` calls `just stop`, builds the debug binary, starts configured
TEI/Chrome services, and runs `axon mcp`. It is a development workflow that
can interrupt an existing local runtime, not a synonym for `axon serve`.
Use `axon serve` for the unified HTTP listener with configured providers.

## Source Layout

```text
crates/axon-mcp/src/
  server.rs                  # canonical dispatch, legacy/atomic/both, auxiliary tools and tasks
  server/authz.rs            # live MCP_ACTION_SPECS allowlist and scopes
  server/tool_schema.rs      # live input schema generation
  server/handlers_source.rs  # action=source
  server/handlers_jobs.rs    # action=jobs
  server/handlers_query.rs   # query/search/research/ask/retrieve/etc.
  server/handlers_watch.rs   # action=watch
  server/handlers_graph.rs   # action=graph
  server/handlers_extract.rs # action=extract
```

Shared domain DTOs live in `axon-api`. Transport-only envelopes and distinct
system/watch requests live in `axon-mcp`; the primary request enum does not
represent the entire catalog. Canonical leaf descriptors come from
`server/operations.rs`, not a separately maintained list of wrappers.

## Development Cycle

1. Edit the shared DTO/service first when behavior crosses transports.
2. Add or update the MCP handler adapter.
3. Add the action to `MCP_ACTION_SPECS` only when it is a real live MCP action.
4. Regenerate/check schemas:

```bash
cargo xtask generated-contracts refresh
cargo xtask generated-contracts check
```

5. Run focused MCP tests:

```bash
cargo test -p axon-mcp tool_schema -- --nocapture
cargo test -p axon-mcp authz -- --nocapture
```

## Adding A Live Action

- Define or reuse an `axon-api` request/result DTO.
- Implement the service entrypoint in `axon-services` or the owning domain
  crate.
- Add the MCP request variant only when the shared DTO must deserialize that
  action.
- Add the action name, scope, description, and cost to `MCP_ACTION_SPECS`.
- Add schema/auth tests proving the action is advertised and scoped.
- Validate legacy, atomic, and combined projections, including rejection of
  fixed routing fields on leaf calls, auxiliary tools, resources, and tasks.
- Preserve caller identity and request metadata through canonicalization.
  A durable job is not automatically a supported MCP task.

Do not add source-family one-off actions. Source acquisition/indexing belongs
under `action=source`.

## Removed Action Guard

Supported `scrape`, `crawl`, `embed`, `ingest`, and `code_search` projections
remain live. These removed names must stay absent:

- `code_search_watch`
- `vertical_scrape`
- `purge`
- `dedupe`

Use `action=source` for indexing and `action=prune` for cleanup.

## HTTP Testing

Use a real MCP client with initialization, negotiated capability/session
headers, and the intended API/OAuth identity. A bare `tools/call` POST is
not a complete protocol smoke test. Follow the [connection guide](connect.md),
discover the actual tool schema, and make a read-only status call.

Loopback-only development may omit a token. Non-loopback binds require
OAuth or `AXON_HTTP_TOKEN`; panel unlock is separate. Include denied
auth/origin cases and preserve job IDs when testing queued operations.
