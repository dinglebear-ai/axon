# axon-web Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-web` owns the REST/OpenAPI and browser web server surface. It maps HTTP
routes, SSE streams, auth, and the web control panel into `axon-services`.

## Owns

- Axum router and route registration
- REST request/response mapping using `axon-api`
- OpenAPI schema export wiring
- SSE progress, job status, health, config, and provider routes
- web panel static asset serving and setup/status endpoints
- HTTP auth middleware integration

## Must Not Own

- source pipeline domain logic
- provider/store/domain internals
- CLI or MCP rendering
- legacy route compatibility aliases

## Public Modules

```text
lib.rs
router.rs
state.rs
routes.rs
openapi.rs
sse.rs
auth.rs
health.rs
assets.rs
error.rs
testing.rs
```

## Public API

- `WebServer`
- `WebAppState`
- `build_router`
- `openapi_document`
- `SseProgressStream`
- route test harness helpers

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-authz`, `axon-observe`,
  `axon-services`
- Axum/Tower/OpenAPI/static asset crates

## Dependencies Forbidden

- direct domain internals bypassing services
- provider clients
- CLI clap types or MCP server types

## Generated Artifacts

- [../../schemas/openapi-schema.md](../../schemas/openapi-schema.md)
- REST route docs in [../../surfaces/rest-contract.md](../../surfaces/rest-contract.md)

## Fixtures And Fakes

- fake service context for route tests
- OpenAPI snapshot fixture
- SSE progress stream fixture
- auth denied fixture

## Tests

- every REST route maps to a shared service request/result
- OpenAPI output is deterministic
- SSE events match `axon-observe` event schema
- removed/compat routes are absent

## Acceptance Criteria

- web/REST is a thin transport over services
- app surfaces can rely on complete route and schema contracts
- route behavior remains aligned with MCP and CLI action contracts

See [../README.md](../README.md) and
[../../surfaces/rest-contract.md](../../surfaces/rest-contract.md).
