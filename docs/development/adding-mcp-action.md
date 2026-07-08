# Adding an MCP Action

`axon-mcp` exposes the entire Axon service surface as a **single** `axon`
tool, action-dispatched (`action` + optional `subaction`), never one MCP tool
per operation. This guide describes the real request-enum + dispatch-match +
handler pattern in `crates/axon-mcp/src/`.

See also: crate guide `crates/axon-mcp/src/CLAUDE.md`, behavior contract
`docs/pipeline-unification/surfaces/tool-contract.md`.

**The core rule:** every action routes to exactly one `axon-services`
entrypoint, and the tool schema is generated from shared `axon-api` DTOs —
never invent a duplicate action DTO or route around `axon-services` into a
domain crate's internals.

## Step 1: Add the request variant

`axon-api::mcp_schema::AxonRequest` (`crates/axon-api/src/mcp_schema.rs`) is
the enum every dispatched action belongs to:

```rust
pub enum AxonRequest {
    Status(StatusRequest),
    Jobs(JobsRequest),
    Memory(MemoryRequest),
    Query(QueryRequest),
    // ...
}
```

Add your new action's request type here (e.g. `YourAction(YourActionRequest)`)
and define `YourActionRequest`/its subaction enum (if the action has
subactions, follow `MemoryRequest`/`MemorySubaction`'s shape) alongside the
other request DTOs in this module — this is the wire schema shared by MCP,
REST, and CLI, not an MCP-only shape.

## Step 2: Write the handler

Handlers live in `crates/axon-mcp/src/server/handlers_<group>.rs` — group by
domain, not one file per action (`handlers_memory.rs`, `handlers_source.rs`,
`handlers_jobs.rs`, `handlers_extract.rs`, `handlers_query.rs`,
`handlers_system.rs`). `handlers_memory.rs::handle_memory` is a clean,
complete reference example:

```rust
impl AxonMcpServer {
    pub(super) async fn handle_memory(
        &self,
        req: MemoryRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let subaction = memory_subaction_label(req.subaction.unwrap_or(MemorySubaction::Remember));
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| internal_error(format!("initialize memory context: {e}")))?;
        let data = memory_svc::dispatch(&ctx, req)
            .await
            .map_err(map_memory_error)?;
        Ok(AxonToolResponse::ok("memory", subaction, data))
    }
}
```

The shape to copy:

1. Build a `ServiceContext` (`self.base_service_context()`), mapping any
   context-construction failure to `internal_error(...)`.
2. Call the matching `axon_services::<domain>::dispatch(&ctx, req)` — the
   handler's only job is to bridge the MCP wire type into the service call,
   never to reimplement logic locally.
3. Map `ClientActionError` (the shared service error type) into the MCP
   `ErrorData` via a small `map_<domain>_error` helper — route retryable/
   internal errors to `internal_error`, everything else to `invalid_params`,
   matching `map_memory_error`'s pattern exactly.
4. Return `AxonToolResponse::ok(<action-name>, <subaction-label>, data)` —
   the shared envelope every action's success response uses.

## Step 3: Wire the dispatch match

`crates/axon-mcp/src/server.rs`'s `axon(...)` tool entrypoint parses the raw
`action`/`subaction` fields, builds an `AxonRequest` via `parse_axon_request`,
then dispatches:

```rust
let response = match request {
    AxonRequest::Status(req) => self.handle_status(req).await?,
    AxonRequest::Memory(req) => self.handle_memory(req).await?,
    // ...
    AxonRequest::YourAction(req) => self.handle_your_action(req).await?,
};
```

Add your new variant's arm here. The match is exhaustive — the compiler will
catch a missing arm the moment you add a new `AxonRequest` variant.

## Removed actions stay in the enum but cannot dispatch

When an action is removed (folded into another action, e.g. `embed`/
`ingest`/`scrape`/`crawl`/`code_search`/`vertical_scrape` folding into
`source`), the pattern is **not** to delete the `AxonRequest` variant — REST
still needs it for backward request-shape compatibility in some paths — but
to reject it before dispatch. `server.rs` keeps the match arm exhaustive with
an explicit comment and error:

```rust
// Removed indexing actions: `embed`, `ingest`, `scrape`, `crawl`,
// `code_search`, and `vertical_scrape` are folded into `source`.
// These variants remain on the shared `AxonRequest` for the REST
// surface, but the MCP authz allow-list rejects them before
// dispatch; the arm here keeps the match exhaustive and gives a
// clear message if one is ever reached.
AxonRequest::Embed(_)
| AxonRequest::Ingest(_)
| AxonRequest::Scrape(_)
| AxonRequest::Crawl(_)
| AxonRequest::CodeSearch(_)
| AxonRequest::VerticalScrape(_) => { /* rejects with a clear error */ }
```

The actual rejection for a removed action happens at the MCP **authz
allow-list** layer, before this match is even reached in the normal case —
see `crates/axon-mcp/src/authz.rs`/`crates/axon-mcp/src/server/authz.rs`.
Follow this same two-layer pattern (allow-list rejection + exhaustive match
arm with a clear message) when retiring an action rather than a bare
`unreachable!()` or silently dropping the variant.

## Step 4: Regenerate the tool schema

The MCP tool's input/output schema (`crates/axon-mcp/src/schema.rs` /
`crates/axon-mcp/src/server/tool_schema.rs`) is generated from the
`axon-api` DTOs, not hand-maintained. After adding a new action/request type:

```bash
just gen-mcp-schema
```

This keeps `docs/reference/mcp/tool-schema.md` (the generated runtime
snapshot) in sync with the actual dispatch surface.

## Step 5: Tests

Add a sidecar `_tests.rs` per the repo convention. Look at
`crates/axon-mcp/src/server/handlers_source_tests.rs` and
`crates/axon-mcp/src/server/tool_schema_tests.rs` for the pattern —
handler-level tests exercise dispatch + error mapping; schema tests assert
generated schema shape and removed-action absence.

```bash
cargo test -p axon-mcp
```

## Boundary reminders

- No source pipeline behavior or provider/store/domain internals in this
  crate — route through `axon-services`.
- No duplicate action DTOs, and no CLI clap types or web router types here.
- No concrete Qdrant/TEI/LLM/SQLite clients.
- Allowed dependencies: `axon-api`, `axon-error`, `axon-core`, `axon-authz`,
  `axon-observe`, `axon-services`, rmcp/MCP transport crates. Forbidden:
  domain crate internals bypassing services, provider clients, the CLI
  command parser, or the web router — enforced by
  `cargo xtask check-layering`.
- Error envelopes must align with REST and CLI JSON output — every response
  is a structured envelope, not ad hoc MCP-only shape.
