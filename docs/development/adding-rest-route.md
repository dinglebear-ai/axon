# Adding a REST Route

`axon-web` owns REST/OpenAPI/SSE and browser web-panel transport — the Axum
router, route registration, OpenAPI export, SSE progress streams, and HTTP
auth middleware, all mapping into `axon-services`. This guide describes the
real handler + router + OpenAPI pattern in `crates/axon-web/src/`.

See also: crate guide `crates/axon-web/src/CLAUDE.md`, behavior contract
`docs/pipeline-unification/surfaces/rest-contract.md`.

**The core rule:** every REST route maps to a shared service request/result —
`axon-web` is a thin transport over `axon-services`. Route handlers never
reach into a domain crate's internals directly.

## Step 1: Write the handler

Handlers live under `crates/axon-web/src/server/handlers/`, one file per
functional group (`sources.rs`, `discovery.rs`, `rag.rs`, `exploration.rs`,
`admin.rs`, `memory.rs`, `jobs.rs`, …), declared in `handlers.rs` via
`#[path = "handlers/<file>.rs"] pub mod <name>;`.

`crates/axon-web/src/server/handlers/sources.rs`'s `index_source` is the
reference example — `POST /v1/sources`, the canonical source-indexing
entrypoint that all the legacy per-family routes fold into:

```rust
#[utoipa::path(
    post,
    path = "/v1/sources",
    request_body = SourceRequest,
    responses(
        (status = 200, description = "Source indexing result", body = SourceResult),
        (status = 400, description = "Invalid source request", body = crate::server::error::ErrorBody),
        (status = 403, description = "Source not authorized for caller scopes", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "sources"
)]
pub(crate) async fn index_source(
    State((state, _cfg)): State<WebState>,
    auth: Option<Extension<AuthContext>>,
    Json(request): Json<SourceRequest>,
) -> Result<Json<SourceResult>, HttpError> {
    // validate -> per-source authorize -> axon_services::index_source -> Json(result)
}
```

Notes on the shape:

- The `#[utoipa::path(...)]` macro is both the OpenAPI doc source **and**
  required for the route to appear in the generated OpenAPI spec — every
  route needs one with an accurate `responses(...)` table (including error
  statuses, using `crate::server::error::ErrorBody`).
- Request/response bodies are `axon-api` DTOs (`SourceRequest`,
  `SourceResult` here) — never invent a REST-only request/response shape.
  Route behavior must stay aligned with the equivalent MCP action and CLI
  command using the same underlying DTOs.
- Validation failures return the contract `ApiError` (via `HttpError::
  from_api_error`), not a bespoke REST error shape — the error envelope is
  shared across transports.
- The handler calls into `axon_services::*` (here,
  `axon_services::source::classify::classify_source_input` for per-source
  auth classification, then the shared `axon_services::index_source` — see
  the handler file's full body) — it does not call a domain crate's internal
  `::ops::*` modules directly.
- If a per-source or per-action authorization boundary is finer than the
  route's broad scope gate (see Step 3), do that check inside the handler,
  as `index_source` does via `ScopeSecurityPolicy::authorize_source`.
- If the underlying service future is not `Send` (e.g. holds a non-`Send`
  error type across an `.await`), run it via `tokio::task::spawn_blocking` +
  `Handle::block_on`, whose `JoinHandle` is `Send` — see the comment at the
  top of `sources.rs` for the concrete pattern used elsewhere in this crate
  (`admin::run_watch` does the same thing).

## Step 2: Register the route in the router

`crates/axon-web/src/server/routing.rs` builds the router from four scope
tiers — `read_routes`, `write_routes`, `large_write_routes`, and
`admin_routes` — each a `Router::new().route("/v1/...", get(...)/post(...))`
chain. Add your route to the tier matching its required scope:

```rust
Router::new()
    .route("/v1/sources", post(handlers::sources::index_source))
    // ...
```

Each tier is wrapped with `protect_routes(..., &auth_policy,
ScopeRequirement::{Read,Write,Admin})` and merged into the final router.
Picking the wrong tier either over-exposes a mutating route on read scope or
needlessly locks a read-only route behind write/admin scope — match the
handler's actual side effects, not its HTTP verb alone (a `POST` search
route can legitimately be `Read` scope if it doesn't mutate state).

## Step 3: Wire OpenAPI

`crates/axon-web/src/server/openapi.rs`'s `paths(...)` macro invocation must
list your handler function so it's included in the generated OpenAPI
document:

```rust
paths(
    // ...
    handlers::sources::index_source,
)
```

Missing this means the route works at runtime but silently disappears from
`/v1/openapi.json` and generated clients — this is a common, easy-to-miss
step.

## Step 4: SSE streams (if applicable)

For long-running or progress-emitting operations, use the `_stream` handler
pattern (`ask_stream.rs`, `chat_stream.rs`, `exploration_stream.rs`,
`jobs_stream.rs`) — SSE events must use the `StreamEvent`/
`SourceProgressEvent` envelopes from `axon-observe`'s event schema, not an
ad hoc SSE payload shape.

## Step 5: Tests

Add a sidecar `_tests.rs` per the repo convention (e.g.
`server_tests.rs`/`server_dedupe_tests.rs` at the server-module level, or a
handler-specific `_tests.rs` such as `ask_tests.rs`,
`chat_stream_tests.rs`, `artifacts_tests.rs`). Test both the route's happy
path and its scope-gating behavior (unauthorized caller → 403).

```bash
cargo test -p axon-web
```

## Boundary reminders

- No source pipeline domain logic or provider/store/domain internals in this
  crate — route through `axon-services`.
- No CLI rendering (clap types) or MCP server types here.
- **No legacy/compat route aliases.** Per the Phase 10 surface cutover
  (`crates/axon-web/src/CLAUDE.md`), direct verb/family routes like
  `/v1/scrape`, `/v1/crawl`, `/v1/embed`, `/v1/ingest` were removed in
  favor of `/v1/sources` — do not add a new alias route for something
  `/v1/sources` (or another canonical end-state route) already covers.
- Allowed dependencies: `axon-api`, `axon-error`, `axon-core`, `axon-authz`,
  `axon-observe`, `axon-services`, Axum/Tower/OpenAPI/static-asset crates.
  Forbidden: domain internals bypassing services, provider clients, CLI
  clap types, MCP server types — enforced by `cargo xtask check-layering`.
- OpenAPI output must stay deterministic; removed/compat routes must be
  absent from the router, OpenAPI, and generated clients — not just hidden.
