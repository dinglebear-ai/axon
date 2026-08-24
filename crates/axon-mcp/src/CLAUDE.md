# axon-mcp — Agent Guide

`axon-mcp` owns the **MCP transport surface**: it exposes the shared action model
as a single `axon` tool, generates the tool schema from `axon-api`, extracts the
caller via `axon-authz`, and maps every call into `axon-services`. Full contract
(owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-mcp/README.md](../../../docs/pipeline-unification/crates/axon-mcp/README.md)
· surface spec:
[../../../docs/pipeline-unification/surfaces/tool-contract.md](../../../docs/pipeline-unification/surfaces/tool-contract.md).

## Status — live unified transport
The single `axon` tool is the live MCP surface. Source acquisition remains under
`action=source`, with focused `scrape`, `crawl`, `embed`, and `ingest` projections
plus read-only committed-state `code_search`. These actions use canonical batch
DTOs and the same source/job services; they are not compatibility pipelines. Artifact-backed
responses return opaque artifact IDs rather than server filesystem paths.

## Module map
Current groups from `crates/axon-mcp/src/`:
| Area | Owns |
|---|---|
| `lib.rs` | crate root — re-exports `AxonMcpServer` + `run_stdio_server` bootstrap |
| `schema_registry.rs` | shared MCP schema-registry helpers |
| `server.rs` + `server/` | MCP server, transport handlers, action routing (target `handler.rs`/`progress.rs`) |
| `schema.rs` | tool input/output schema generated from `axon-api` (target `tool_model.rs`) |
| `auth.rs` | caller extraction / auth wiring via `axon-authz` |
| `cors.rs` | HTTP-transport CORS/origin handling |
| `assets` | static/schema assets |

## Boundary — keep OUT of this crate
- Source pipeline behavior, provider/store/domain internals — route through `axon-services`.
- Duplicate action DTOs; CLI clap types or web router types.
- Concrete Qdrant/TEI/LLM/SQLite clients.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-authz`, `axon-observe`, `axon-services`, rmcp/MCP transport crates.
- **Forbidden:** domain crate internals bypassing services, provider clients, the CLI command parser or web router. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- One action-dispatched `axon` tool (`action` + optional `subaction`) — never one tool per operation.
- Every action routes to exactly one `axon-services` entrypoint; tool schema is generated from shared `axon-api` DTOs.
- Error envelopes align with REST and CLI JSON output; every response returns a structured envelope.
- Removed actions such as `code_search_watch`, purge, dedupe, and `vertical_scrape` remain absent; destructive reset stays under `action=reset` with admin scope.

## DTO ownership
Wire DTOs and the response envelope live in **`axon-api`** (`axon_api::mcp_schema`
lineage); this crate generates its schema from them and returns them. Transports
call `axon-services`/`axon-api`, never a domain crate's `::ops::*` or internals.

## Keep in sync when shapes change
`README.md` (crate contract) · `surfaces/tool-contract.md` ·
`schemas/mcp-tool-schema.md` · the action/request/result and envelope DTOs in
`axon-api`.
