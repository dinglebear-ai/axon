# axon-mcp Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-mcp` owns the MCP transport surface for Axon. It exposes the shared action
model through MCP tools and maps all calls into `axon-services`.

## Owns

- MCP server bootstrap and transport handlers
- single action-dispatched tool model
- tool input/output schema generation from `axon-api`
- MCP auth/caller extraction integration with `axon-authz`
- MCP progress/status response mapping

## Must Not Own

- source pipeline behavior
- duplicate action DTOs
- provider/store/domain internals
- CLI or REST compatibility aliases

## Public Modules

```text
lib.rs
server.rs
tool_model.rs
schema.rs
handler.rs
auth.rs
progress.rs
error.rs
testing.rs
```

## Public API

- `McpServer`
- `McpToolModel`
- `McpActionHandler`
- `McpSchemaDocument`
- `McpCallerExtractor`
- `run_mcp_server`
- MCP test harness helpers

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-authz`, `axon-observe`,
  `axon-services`
- rmcp/MCP transport crates

## Dependencies Forbidden

- domain crate internals bypassing services
- Qdrant/TEI/LLM/SQLite concrete clients
- CLI command parser or web router dependencies

## Generated Artifacts

- [../../schemas/mcp-tool-schema.md](../../schemas/mcp-tool-schema.md)
- tool docs in [../../surfaces/tool-contract.md](../../surfaces/tool-contract.md)

## Fixtures And Fakes

- MCP request/response fixtures for every action
- auth denied fixture
- progress/status fixture
- schema snapshot fixture

## Tests

- MCP schema is generated from shared DTOs
- every tool action calls exactly one service entrypoint
- error envelopes match REST/CLI semantics
- no removed/compat action aliases are present

## Acceptance Criteria

- MCP is a thin transport over the shared source/action model
- tool schema is complete enough for generated clients and agents
- MCP does not drift from command and REST contracts

See [../README.md](../README.md) and
[../../surfaces/tool-contract.md](../../surfaces/tool-contract.md).
