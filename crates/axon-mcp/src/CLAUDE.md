# axon-mcp — Agent Guide

`axon-mcp` owns the **MCP transport surface**: it exposes the shared action model
as a single `axon` tool, generates the tool schema from `axon-api`, extracts the
caller via `axon-authz`, and maps every call into `axon-services`. Full contract
(owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-mcp/README.md](../../../docs/pipeline-unification/crates/axon-mcp/README.md)
· surface spec:
[../../../docs/pipeline-unification/surfaces/tool-contract.md](../../../docs/pipeline-unification/surfaces/tool-contract.md).

## Status — live crate, cutover at Phase 10
The single `axon` tool is the live MCP surface. Source acquisition is under
`action=source`; removed indexing actions are omitted from the live action enum
and rejected at dispatch. Job-kind DTOs may still mention crawl/embed/ingest for
status/backcompat metadata, but they are not callable MCP actions. Responses are
still being tightened toward the full shared envelope.

## Module map
Current groups from `crates/axon-mcp/src/`:
| Area | Owns |
|---|---|
| `lib.rs` | crate root + `run_mcp_server` bootstrap |
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
- Removed actions are absent from the schema and cannot dispatch after the clean break; destructive reset stays under `action=reset` with admin scope.

## DTO ownership
Wire DTOs and the response envelope live in **`axon-api`** (`axon_api::mcp_schema`
lineage); this crate generates its schema from them and returns them. Transports
call `axon-services`/`axon-api`, never a domain crate's `::ops::*` or internals.

## Keep in sync when shapes change
`README.md` (crate contract) · `surfaces/tool-contract.md` ·
`schemas/mcp-tool-schema.md` · the action/request/result and envelope DTOs in
`axon-api`.
