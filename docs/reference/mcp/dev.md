# MCP Development Workflow -- Axon

Day-to-day development guide for the Axon MCP server.

## Quick Start

```bash
git clone https://github.com/jmagar/axon.git
cd axon
mkdir -m 700 -p ~/.axon
cp .env.example ~/.axon/.env
# Edit ~/.axon/.env with service URLs/secrets.

just dev
```

`just dev` starts infrastructure and `axon serve`, including MCP HTTP on the
unified listener.

## Source Layout

```text
crates/axon-mcp/src/
  server.rs                  # AxonMcpServer, single-tool dispatch
  server/authz.rs            # live MCP_ACTION_SPECS allowlist and scopes
  server/tool_schema.rs      # live input schema generation
  server/handlers_source.rs  # action=source
  server/handlers_jobs.rs    # action=jobs
  server/handlers_query.rs   # query/search/research/ask/retrieve/etc.
  server/handlers_watch.rs   # action=watch
  server/handlers_graph.rs   # action=graph
  server/handlers_extract.rs # action=extract
```

The shared MCP request DTOs live in `crates/axon-api/src/mcp_schema/`.

## Development Cycle

1. Edit the shared DTO/service first when behavior crosses transports.
2. Add or update the MCP handler adapter.
3. Add the action to `MCP_ACTION_SPECS` only when it is a real live MCP action.
4. Regenerate/check schemas:

```bash
cargo xtask schemas generate --update-fixtures
cargo xtask schemas generate --check
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

Do not add source-family one-off actions. Source acquisition/indexing belongs
under `action=source`.

## Removed Action Guard

These names must stay absent from the live MCP schema and action allowlist:

- `scrape`
- `crawl`
- `embed`
- `ingest`
- `code_search`
- `code_search_watch`
- `vertical_scrape`
- `purge`
- `dedupe`

Use `action=source` for indexing and `action=prune` for cleanup.

## HTTP Testing

```bash
curl -X POST http://localhost:8001/mcp \
  -H "Authorization: Bearer $AXON_HTTP_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"axon","arguments":{"action":"doctor"}}}'
```

Loopback development binds may omit the token. Non-loopback binds require OAuth
or `AXON_HTTP_TOKEN`.
