# MCP Code Patterns -- Axon

Axon MCP uses one operation tool, `axon`, with `action`/`subaction` routing.
MCP handlers are transport adapters over shared `axon-api` DTOs and
`axon-services` entrypoints.

## Dispatch Pattern

```rust
match request {
    AxonRequest::Source(req) => self.handle_source(req).await?,
    AxonRequest::Query(req) => self.handle_query(req).await?,
    AxonRequest::Retrieve(req) => self.handle_retrieve(req).await?,
    AxonRequest::Jobs(req) => self.handle_jobs(req).await?,
    AxonRequest::Prune(req) => self.handle_prune(req).await?,
}
```

The live action allowlist is `MCP_ACTION_SPECS` in
`crates/axon-mcp/src/server/authz.rs`. Removed action variants are absent from
the request DTO and generated schema; unknown action names fail parsing before
handler dispatch.

## Source Indexing

`action=source` replaces the removed `scrape`, `crawl`, `embed`, `ingest`,
`code_search`, and `vertical_scrape` MCP actions. It maps to `SourceRequest`:

```json
{ "action": "source", "source": "https://example.com", "scope": "page", "embed": true }
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

Source, extract, watch-triggered, memory, and operational work share the unified
job/event model. Source watches enqueue canonical Source jobs and record those
job IDs in watch-run history.

## Response Modes

Handlers support `artifact`, `inline`, `both`, and `auto_inline` where the result
shape can be artifact-backed. Artifact responses contain opaque `artifact_id`
references, never server paths. `retrieve` is the document-reading exception
and defaults to inline-first paged content.
