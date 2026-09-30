# MCP Code Patterns -- Axon

Axon MCP uses one canonical dispatcher with legacy aggregate `axon`,
atomic leaf, and combined projections, plus an auxiliary dashboard tool.
MCP handlers are transport adapters over shared `axon-api` DTOs and
`axon-services` entrypoints.

## Dispatch Pattern

```text
presented tool + arguments + metadata
  -> canonical operation identity and policy checks
  -> typed primary, system, or watch request
  -> existing domain handler -> shared service
  -> typed result/error and permitted response projection
```

This is a responsibility sketch, not an exhaustive Rust match. Inspect
[server.rs](../../../crates/axon-mcp/src/server.rs) and
[projection_call.rs](../../../crates/axon-mcp/src/server/projection_call.rs)
for actual routing order. Atomic leaves reject fixed selector fields and
reuse handlers instead of calling aggregate MCP internally.

The live action allowlist is `MCP_ACTION_SPECS` in
`crates/axon-mcp/src/server/authz.rs`. Removed action variants are absent from
the request DTO and generated schema; unknown action names fail parsing before
handler dispatch.

## Source Indexing

`action=source` remains the universal source surface. Focused `scrape`,
`crawl`, `embed`, and `ingest` projections map to the same `SourceRequest`
pipeline; read-only `code_search` queries committed local-code vectors.
`vertical_scrape` remains removed. The universal source shape is:

<!-- doc-example: kind=json schema=mcp/tool-schema.json#/$defs/AxonToolInput -->
```json
{ "action": "source", "source": "https://example.com", "scope": "page" }
```

The source handler calls `axon_services::source`/`index_source` and receives a
transport-neutral `SourceResult`.

## Services Layer

All MCP handlers call services, not infrastructure directly:

```text
MCP handler -> axon-services -> domain/adapters -> axon-api result DTO
```

Service functions return typed results. Handlers are responsible only for MCP
auth, request conversion, response-mode handling, and error mapping.

## Error Mapping

| Condition | MCP error |
|---|---|
| Unknown/removed action | `invalid_params` |
| Invalid subaction | `invalid_params` |
| Missing required field | `invalid_params` |
| Provider/service failure | `internal_error` |
| Authorization failure | `invalid_request` with required scope |

## Jobs Pattern

Durable async work is surfaced through `action=jobs`, not through one action
per source or operation kind. Use `subaction=list|get|events|stream|cancel|retry|recover|
cleanup|clear`.

Source, extract, watch-triggered, memory, and operational work share the
unified job/event model. Preserve the returned job IDs and inspect watch-run
history rather than inferring job kind from the source family.

Protocol tasks require separate capability negotiation; only extraction
start currently supports task augmentation. Preserve caller identity and
progress metadata, and reject unsupported task calls before enqueue.

## Response Modes

Handlers support `artifact`, `inline`, `both`, and `auto_inline` where the result
shape can be artifact-backed. Artifact responses contain opaque `artifact_id`
references, never server paths. `retrieve` is the document-reading exception
and defaults to inline-first paged content.
