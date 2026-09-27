# axon-web — Agent Guide

`axon-web` owns the **REST / OpenAPI / SSE and browser web-panel transport**: the
Axum router, route registration, OpenAPI export, SSE progress streams, HTTP auth
middleware, and static panel serving — all mapping into `axon-services`. Full
contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-web/README.md](../../../docs/pipeline-unification/crates/axon-web/README.md)
· surface spec:
[../../../docs/pipeline-unification/surfaces/rest-contract.md](../../../docs/pipeline-unification/surfaces/rest-contract.md).

## Status — focused projections over the unified pipeline
The router exposes canonical `/v1/sources`, `/v1/jobs`, and `/v1/prune/*`
resources plus focused `/v1/scrape`, `/v1/crawl`, `/v1/embed`, `/v1/ingest`,
and read-only `/v1/code-search` projections. The focused routes share DTOs,
preflight, admission, and services with CLI/MCP; `/v1/purge`, `/v1/dedupe`, and
family-scoped job routes remain absent.

## Module map
Current groups from `crates/axon-web/src/`:
| Area | Owns |
|---|---|
| `lib.rs` | crate root — re-exports `router`, `PanelRuntimeState`, `openapi_document()` |
| `schema_registry.rs` + `schema_registry/` | REST route schema-registry helpers (admin/watch/extract/graph/memory routes) |
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
