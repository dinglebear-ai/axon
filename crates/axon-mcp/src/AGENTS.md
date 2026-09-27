# axon-mcp — Agent Guide

This scoped guide extends the root AGENTS.md. CLAUDE.md and GEMINI.md are
direct relative aliases of this file. Last reviewed: 2026-09-27.

axon-mcp owns the MCP transport: tool discovery, request routing, task
protocol handling, resources, auth integration, and transport envelopes.
Domain operations remain in typed services and their owning crates.

## Current catalog and contracts

The primary axon tool uses action/subaction routing. The catalog also contains
axon_status_dashboard, an auxiliary MCP App tool. Source acquisition and the
focused scrape/crawl/embed/ingest projections share the canonical source/job
services; code_search queries committed state.

server.rs owns the advertised tool catalog and routes. The primary request
schema is assembled by server/tool_schema.rs. Ordinary action DTOs route
through AxonRequest; system operations including reset, collections, uploads,
and artifacts use McpSystemRequest; watch uses McpWatchRequest. Do not infer
that reset is absent merely because it is outside the narrower AxonRequest
enum. Preserve validation and confirmation requirements in those request types.

The [generated MCP reference](../../../docs/reference/mcp/tool-schema.md)
and matching runtime tools/list describe the current surface. The
[design packet](../../../docs/pipeline-unification/surfaces/tool-contract.md)
is historical design context, not permission to remove working runtime tools.
Unmerged atomic-projection work must not be documented as main behavior.

## Module map

| Area | Responsibility |
|---|---|
| lib.rs | Crate exports and bootstrap |
| server.rs | Server composition, tool registration, and dispatch |
| server/handlers_*.rs | Typed operation handlers |
| server/tool_schema.rs and schema.rs | Runtime input schema and MCP action router |
| server/system_requests.rs | System/watch transport request types |
| server/tasks.rs and task_* modules | MCP task lifecycle and progress |
| server/handler_meta.rs and assets/ | Resource and MCP App metadata/assets |
| auth.rs and server/authz.rs | Caller extraction and authorization |
| cors.rs, server/http.rs, server/stdio.rs | Transport-specific wiring |

## Boundaries and invariants

- Keep source pipeline behavior, provider retries, store clients, and domain
  internals out of this crate. Use the shared service boundaries.
- Shared operation DTOs belong in axon-api; the MCP-only tagged router and
  envelopes belong here. Do not duplicate clap or Axum request types.
- Keep tool discovery, runtime dispatch, schemas, auth requirements, task
  behavior, and error envelopes consistent. An auxiliary tool must not bypass
  the same security and service rules as the primary tool.
- Return opaque artifact IDs, not server filesystem paths. Preserve structured
  envelopes and stdout/stderr separation for protocol clients.
- Do not restore removed code_search_watch, purge, dedupe, or vertical_scrape
  pipelines. New projections must reuse canonical services and job semantics.

## Verification

Use the neighboring sidecar tests for schema, discovery, routing, authorization,
and task changes. Update generator inputs, then run generated-contracts refresh
and check; do not hand-edit generated Markdown. Exercise real tools/list and
representative calls against the matching build when changing the wire surface.
A mock-only test does not prove compatibility with an actual MCP client.
