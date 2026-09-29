# axon-web

Own Axum routes, REST/OpenAPI, SSE, panel/static assets, HTTP authentication, and health/readiness projections.

## Read before changing

[adding rest route](../../../docs/development/adding-rest-route.md) · [http api](../../../docs/reference/http-api.md) · [api parity](../../../docs/reference/api-parity.md) · [security](../../../docs/operations/security.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[server.rs](server.rs) · [server/](server/) · [auth.rs](auth.rs) · [health.rs](health.rs) · [security.rs](security.rs) · [panel_first_run.rs](panel_first_run.rs) · [panel_stack.rs](panel_stack.rs) · [static_assets.rs](static_assets.rs)

## Change requirements

- Route typed requests through shared services. Keep router registration, OpenAPI/client artifacts, responses, and authorization aligned rather than documenting routes that only exist in a DTO.

- Preserve separate panel password/session and API/MCP authentication boundaries; test missing/invalid credentials and origin rules. Do not expose config secrets to browser bundles.

- SSE uses shared event/correlation semantics; cancellation, disconnect, and partial failures must not become silent successful streams.

- healthz is not interchangeable with readyz. Document the actual dependency readiness results; a healthy container or HTTP listener alone does not prove Qdrant/TEI readiness. Static/dev fallback assets do not prove a production panel build.

## Verification for code changes

Server route/auth/security/health sidecars, HTTP/MCP parity, SSE failure/cancellation cases, and OpenAPI regeneration checks.

Use focused `cargo test -p axon-web` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
