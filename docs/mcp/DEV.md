# MCP Development Workflow -- Axon

Day-to-day development guide for the Axon MCP server.

## Quick start

```bash
git clone https://github.com/jmagar/axon.git
cd axon_rust
cp .env.example .env && chmod 600 .env
# Edit .env with your credentials

just dev          # Starts infrastructure + axon serve (includes MCP HTTP on port 8001)
```

## MCP source structure

```
crates/
├── mcp.rs              # Module root — re-exports schema + server
├── mcp/
│   ├── schema.rs       # Tool input schema, action/subaction enums, serde parsing
│   └── server.rs       # AxonMcpServer, handler dispatch, HTTP/stdio transport setup
├── services.rs         # Module root — service layer exports
├── services/
│   ├── context.rs      # ServiceContext — shared state for all handlers
│   ├── types/
│   │   └── service.rs  # Typed result structs returned by all service functions
│   └── ...             # Per-domain service functions (query, ask, sources, etc.)
```

The MCP server calls the services layer, which is the same layer used by CLI handlers and web routes. MCP handlers map typed service results to MCP wire format.

## Development cycle

1. **Edit source** -- modify handlers in `crates/mcp/server.rs` or service functions in `crates/services/`
2. **Build** -- `cargo check` or `just check` for fast feedback
3. **Test** -- `cargo test` or `just test`
4. **Run** -- `just dev` starts the full stack including MCP HTTP
5. **Verify** -- `just mcp-smoke` runs the MCP smoke test suite

## Adding a new action

### Direct action (no subaction)

1. **Add enum variant** in `crates/mcp/schema.rs`:
   - Add to the action enum
   - Define required and optional parameters

2. **Implement service function** in `crates/services/`:
   - Create a function that returns a typed result struct
   - Define the result struct in `crates/services/types/service.rs`

3. **Add CLI handler** in `crates/cli/commands/`:
   - Create a new command file
   - Call the service function, format output for stdout

4. **Wire MCP dispatch** in `crates/mcp/server.rs`:
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

Lifecycle actions (crawl, extract, embed, ingest, refresh) follow a common pattern with `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover` subactions.

1. Use the existing `crates/jobs/` framework
2. Implement a `Processor` trait for the new job type
3. Add a worker binary path
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
# Health check
curl -s http://localhost:8001/health

# Tool call
curl -X POST http://localhost:8001/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"axon","arguments":{"action":"doctor"}}}'

# List tools
curl -X POST http://localhost:8001/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

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
RUST_LOG=info,axon::crates::mcp=debug axon mcp
```

### Response artifacts

When `response_mode=path` (default), responses are written to `$AXON_MCP_ARTIFACT_DIR`. Inspect artifacts:

```bash
ls -la .cache/axon-mcp/
cat .cache/axon-mcp/latest-response.json | jq .
```

### Service context

The `ServiceContext` in `crates/services/context.rs` carries a `ServiceCapabilities` struct that gates operations based on the runtime mode (full vs lite). Check capabilities before executing:

```rust
if !ctx.capabilities.jobs.supported {
    return Err("Operation requires full mode (Postgres + RabbitMQ)".into());
}
```

## See also

- [TOOLS.md](TOOLS.md) -- action/subaction reference
- [PATTERNS.md](PATTERNS.md) -- code patterns
- [../repo/RECIPES.md](../repo/RECIPES.md) -- Justfile recipes
