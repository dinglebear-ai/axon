# src/mcp
Last Modified: 2026-03-10

Axon MCP server crate backing the `axon mcp` command.

## Scope
- MCP transport and server wiring (`server.rs`)
- Tool request schema and strict parser (`schema.rs`)
- Runtime config loading (`config.rs`)

## Public Contract
- Single MCP tool: `axon`
- Transport: `http`, `stdio`, or `both` via `axon mcp --transport ...`
- Primary request shape: action-routed requests via `action` + `subaction`
- Parser is strict (no fallback action keys, no alias remapping)
- Context-safe default: large payloads written artifact-first to `~/.axon/artifacts/<context>/` (small payloads return inline)
- Resource exposed: `axon://schema/mcp-tool`
- MCP App resource exposed: `ui://axon/status-dashboard`
- MCP Apps capability is advertised so compatible hosts can render the dashboard widget

See source-of-truth docs:
- `docs/reference/mcp/overview.md`
- `docs/reference/mcp/tool-schema.md`

## Local Development
```bash
cargo check --bin axon
cargo run --bin axon -- mcp
cargo run --bin axon -- mcp --transport stdio
cargo run --bin axon -- mcp --transport both
```

HTTP MCP transport is the default, and the CLI also supports stdio-only and dual-transport modes. See `docs/reference/mcp/overview.md`.

## Schema Validation / Smoke Tests
Primary MCP smoke path:

```bash
bash ./scripts/test-mcp-tools-mcporter.sh
```

```bash
mcporter --config config/mcporter.json list axon --schema
mcporter --config config/mcporter.json call axon.axon action:doctor --output json
mcporter --config config/mcporter.json call axon.axon action:source source:https://example.com scope:page embed:true --output json
```

The smoke harness uses SQLite/in-process jobs. Legacy source-family actions
such as `crawl`, `scrape`, `embed`, and `ingest` were removed from MCP; use
`action=source`.

## Change Rule
When changing tool behavior, update in the same commit:
1. `src/mcp/schema.rs`
2. `src/mcp/server.rs`
3. `docs/reference/mcp/overview.md`
4. `docs/reference/mcp/tool-schema.md`

## Related Docs
- [Repository README](../../../README.md)
- [Architecture](../../../docs/architecture/overview.md)
- [MCP Runtime Guide](../../../docs/reference/mcp/overview.md)
