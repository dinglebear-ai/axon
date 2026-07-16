# axon-web — Agent Guide

`axon-web` owns the **REST / OpenAPI / SSE and browser web-panel transport**: the
Axum router, route registration, OpenAPI export, SSE progress streams, HTTP auth
middleware, and static panel serving — all mapping into `axon-services`. Full
contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-web/README.md](../../../docs/pipeline-unification/crates/axon-web/README.md)
· surface spec:
[../../../docs/pipeline-unification/surfaces/rest-contract.md](../../../docs/pipeline-unification/surfaces/rest-contract.md).

## Status — live crate, post Phase 10 cutover
The router exposes the clean-break REST surface: source lifecycle flows through
`/v1/sources`, job lifecycle flows through `/v1/jobs`, and prune flows through
`/v1/prune/*`. Removed direct verb/family routes (`/v1/scrape`, `/v1/crawl`,
`/v1/embed`, `/v1/ingest`, `/v1/purge`, `/v1/dedupe`, and family-scoped job
routes such as `/v1/extract/{id}`) are absent from the router, OpenAPI, and
generated clients. Do not add legacy route aliases.

## Module map
Current groups from `crates/axon-web/src/`:
| Area | Owns |
|---|---|
| `lib.rs` | crate root + web server bootstrap (`WebServer`) |
| `server.rs` + `server/` | Axum router build, route registration, app state (target `router.rs`/`routes.rs`/`state.rs`/`openapi.rs`/`sse.rs`) |
| `auth.rs` | HTTP auth middleware integration (`axon-authz`) |
| `security.rs` | security headers / hardening |
| `health.rs` · `metrics.rs` | health + metrics routes |
| `panel_first_run.rs` · `panel_stack.rs` · `static_assets.rs` | web control-panel setup/status + static asset serving |

## Boundary — keep OUT of this crate
- Source pipeline domain logic, provider/store/domain internals — route through `axon-services`.
- CLI rendering (clap types) or MCP server types.
- Legacy/compat route aliases.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-authz`, `axon-observe`, `axon-services`, Axum/Tower/OpenAPI/static-asset crates.
- **Forbidden:** domain internals bypassing services, provider clients, CLI clap types, MCP server types. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- Every REST route maps to a shared service request/result; web/REST is a thin transport over services.
- OpenAPI output is deterministic; removed/compat routes are absent from router, OpenAPI, and generated clients.
- SSE events use `StreamEvent`/`SourceProgressEvent` envelopes matching the `axon-observe` event schema.
- Route behavior stays aligned with the MCP and CLI action contracts (same shared DTOs/envelopes).

## DTO ownership
Request/response bodies and stream envelopes live in **`axon-api`**; this crate
serializes and returns them and mounts the generated OpenAPI. Transports call
`axon-services`/`axon-api`, never a domain crate's `::ops::*` or internals.

## Keep in sync when shapes change
`README.md` (crate contract) · `surfaces/rest-contract.md` ·
`surfaces/web-contract.md` · `schemas/openapi-schema.md` · the route
request/result DTOs and stream envelopes in `axon-api`.
