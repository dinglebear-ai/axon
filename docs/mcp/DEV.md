# MCP Development Workflow -- Axon

Day-to-day development guide for the Axon MCP server.

## Quick start

```bash
git clone https://github.com/jmagar/axon.git
cd axon_rust
mkdir -m 700 -p ~/.axon
cp .env.example ~/.axon/.env && chmod 600 ~/.axon/.env
# Edit ~/.axon/.env with your credentials

just dev          # Starts infrastructure + axon serve (includes MCP HTTP on port 8001)
```

## MCP source structure

```
src/
├── mcp.rs                           # Crate-root re-exports
├── mcp/
│   ├── schema.rs                    # AxonRequest / AxonToolResponse + action/subaction enums
│   ├── server.rs                    # AxonMcpServer + tool dispatch + stdio entry point
│   ├── auth.rs                      # AuthPolicy, lab-auth OAuth/JWT, static bearer/x-api-key
│   ├── cors.rs                      # AXON_MCP_ALLOWED_ORIGINS CORS middleware
│   └── server/
│       ├── http.rs                  # Streamable HTTP server + auth + host allowlist
│       ├── common.rs                # Shared handler utilities
│       ├── handlers_crawl_extract.rs
│       ├── handlers_embed_ingest.rs
│       ├── handlers_query.rs        # query/retrieve/search/map/scrape/ask/research/screenshot
│       ├── handlers_system.rs       # doctor/domains/sources/stats/status/artifacts/help
│       ├── handlers_elicit.rs       # elicit_demo handler
│       ├── artifacts/               # Artifact storage helpers
│       └── handlers_system/         # Sub-handlers (e.g. screenshot.rs)
├── services.rs                      # Service-layer module root
├── services/
│   ├── context.rs                   # ServiceContext { cfg, jobs }
│   ├── runtime.rs                   # ServiceJobRuntime trait + LiteServiceRuntime
│   ├── types/service.rs             # Typed result structs
│   └── ...                          # query, ask, system, crawl, embed, ingest, etc.
```

The MCP server calls the services layer, which is the same layer used by CLI handlers and web routes. MCP handlers map typed service results to MCP wire format.

## Development cycle

1. **Edit source** -- modify handlers in `src/mcp/server.rs` or service functions in `src/services/`
2. **Build** -- `cargo check` or `just check` for fast feedback
3. **Test** -- `cargo test` or `just test`
4. **Run** -- `just dev` starts the full stack including MCP HTTP
5. **Verify** -- `just mcp-smoke` runs the MCP smoke test suite

## Adding a new action

### Direct action (no subaction)

1. **Add enum variant** in `src/mcp/schema.rs`:
   - Add to the action enum
   - Define required and optional parameters

2. **Implement service function** in `src/services/`:
   - Create a function that returns a typed result struct
   - Define the result struct in `src/services/types/service.rs`

3. **Add CLI handler** in `src/cli/commands/`:
   - Create a new command file
   - Call the service function, format output for stdout

4. **Wire MCP dispatch** in `src/mcp/server.rs`:
   - Add match arm for the new action
   - Call the service function
   - Map the typed result to MCP response format

5. **Regenerate schema doc**:
   ```bash
   just gen-mcp-schema
   ```

6. **Add tests**:
   - Unit test for the service function
   - Add to `scripts/test-mcp-tools-mcporter.sh` smoke test

### Lifecycle action (with subactions)

Lifecycle actions (crawl, extract, embed, ingest) follow a common pattern with
`start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover` subactions.

1. Use the existing `src/jobs/` framework
2. Implement a `Processor` trait for the new job type
3. Wire the in-process worker into `LiteBackend::new_with_workers`
4. Wire into MCP dispatch with subaction routing

## Testing

### Unit tests

```bash
cargo test                    # All tests
cargo test mcp                # MCP-specific tests
```

### MCP smoke tests

```bash
just mcp-smoke
# or directly:
./scripts/test-mcp-tools-mcporter.sh
```

The smoke test starts an MCP server, sends tool calls, and verifies responses.

### curl testing (HTTP transport)

```bash
# Tool call (loopback bind without a token works; non-loopback requires
# AXON_MCP_HTTP_TOKEN — pass it via Authorization or x-api-key)
curl -X POST http://localhost:8001/mcp \
  -H "Authorization: Bearer $AXON_MCP_HTTP_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"axon","arguments":{"action":"doctor"}}}'

# List tools
curl -X POST http://localhost:8001/mcp \
  -H "Authorization: Bearer $AXON_MCP_HTTP_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

The MCP HTTP server does not expose a dedicated `/health` endpoint; a `200`
response on `tools/list` is the canonical liveness check.

### MCP Inspector

```bash
npx @modelcontextprotocol/inspector
```

Connect to `http://localhost:8001/mcp`.

## Code style

| Tool | Command | Purpose |
|------|---------|---------|
| clippy | `cargo clippy --all-targets` | Lint |
| rustfmt | `cargo fmt` | Format |
| cargo check | `cargo check` | Type check |
| cargo test | `cargo test` | Run tests |

Run all checks:

```bash
just verify      # fmt-check + clippy + check + test
just fix         # auto-fix: fmt + clippy --fix
```

## Debugging

### Log levels

Set `RUST_LOG` for MCP-specific filtering:

```bash
RUST_LOG=info,axon::axon::mcp=debug axon mcp
```

### Response artifacts

When `response_mode=path` (default), responses are written to `$AXON_MCP_ARTIFACT_DIR`. Inspect artifacts:

```bash
ls -la .cache/axon-mcp/
cat .cache/axon-mcp/latest-response.json | jq .
```

### Service context

The `ServiceContext` in `src/services/context.rs` carries `cfg` and `jobs`
fields only. There is no `ServiceCapabilities` struct on the context. The
SQLite/in-process job runtime is always available. Operations that
were previously gated (graph, refresh, watch scheduler) have either been
removed or now return runtime errors when their backing service is missing.

## See also

- [TOOLS.md](TOOLS.md) -- action/subaction reference
- [PATTERNS.md](PATTERNS.md) -- code patterns
- [../repo/RECIPES.md](../repo/RECIPES.md) -- Justfile recipes
