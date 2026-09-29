# axon-web design-contract maintenance

This directory documents the `axon-web` boundary: Own Axum routes, REST/OpenAPI, SSE, panel/static assets, HTTP authentication, and health/readiness projections.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-web/src/AGENTS.md) · [Crate exports](../../../../crates/axon-web/src/lib.rs)

[adding rest route](../../../../docs/development/adding-rest-route.md) · [http api](../../../../docs/reference/http-api.md) · [api parity](../../../../docs/reference/api-parity.md) · [security](../../../../docs/operations/security.md)

## Review the actual boundary

- Route typed requests through shared services. Keep router registration, OpenAPI/client artifacts, responses, and authorization aligned rather than documenting routes that only exist in a DTO.

- healthz is not interchangeable with readyz. Document the actual dependency readiness results; a healthy container or HTTP listener alone does not prove Qdrant/TEI readiness. Static/dev fallback assets do not prove a production panel build.

For implementation evidence, inspect [server.rs](../../../../crates/axon-web/src/server.rs), [server/](../../../../crates/axon-web/src/server/), [auth.rs](../../../../crates/axon-web/src/auth.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

Server route/auth/security/health sidecars, HTTP/MCP parity, SSE failure/cancellation cases, and OpenAPI regeneration checks.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
