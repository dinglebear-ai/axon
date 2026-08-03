---
title: "Endpoint Discovery"
updated: 2026-08-02
---

# Endpoint Discovery

`axon endpoints` discovers HTTP, API, RPC, MCP, and ACP candidates from one
page, its first-party JavaScript bundles, and optional Chrome network capture.
It can verify HTTP candidates and probe RPC protocol behavior without
credentials.

## CLI

```bash
axon endpoints https://example.com
axon endpoints https://example.com --include-bundles true --json
axon endpoints https://example.com --verify --probe-rpc --json
axon endpoints https://app.example.com --capture-network --json
```

Options include bundle scanning, first-party filtering, deduplication, script
and byte limits, HTTP verification, Chrome capture, RPC probing, and optional
`mcp.<apex>` candidate probing. The installed binary's
`axon endpoints --help` output is authoritative.

## MCP

```json
{
  "action": "endpoints",
  "url": "https://example.com",
  "include_bundles": true,
  "verify": true,
  "probe_rpc": true
}
```

The request DTO is
`crates/axon-api/src/mcp_schema/requests.rs::EndpointsRequest`. The handler is
`crates/axon-mcp/src/server/handlers_query.rs::handle_endpoints`.

## REST

`POST /v1/endpoints` accepts the same operational fields. Implementation:

- route: `crates/axon-web/src/server/routing.rs`
- handler: `crates/axon-web/src/server/handlers/exploration.rs`
- OpenAPI registry: `crates/axon-web/src/server/openapi.rs`

The generated [OpenAPI](rest/openapi.md) and [route inventory](rest/routes.md)
are authoritative for the HTTP wire contract.

## Service pipeline

The shared implementation is `crates/axon-services/src/endpoints.rs` and its
submodules:

```text
normalized target URL
  -> DNS-aware SSRF validation
  -> bounded page fetch
  -> HTML endpoint extraction
  -> optional first-party bundle discovery/fetch
  -> optional Chrome network capture
  -> normalize, classify, merge, deduplicate
  -> optional unauthenticated verification
  -> optional JSON-RPC/MCP/ACP probing
  -> EndpointReport
```

Current modules:

- `endpoints/fetch.rs`: bounded page and bundle fetching
- `endpoints/capture.rs`: Chrome network capture
- `endpoints/candidates.rs`: RPC/MCP candidate synthesis and merging
- `endpoints/verify.rs`: HTTP verification
- `endpoints/probe.rs`: JSON-RPC/MCP/ACP probes

Core extraction and shared types are owned by
`crates/axon-core/src/content/endpoints.rs`. Services re-export the types
through `crates/axon-services/src/types/endpoints.rs`.

## Result model

`EndpointReport` includes discovered records, source classification,
normalization, host counts, verification outcomes, RPC probe results, truncation
indicators, warnings/errors, and elapsed work metadata.

Evidence can originate from page HTML, script bundles, Chrome network capture,
or synthesized protocol candidates. Endpoint kinds classify the HTTP/API,
WebSocket, GraphQL, JSON-RPC, MCP, ACP, and related protocols supported by the
current extractor.

## Limits and concurrency

Defaults come from Axon configuration. The service also applies process-wide
limits for concurrent bundle fetches, Chrome sessions, response sizes, captured
request counts, and validation/probe concurrency. Use explicit request caps for
untrusted or very large sites.

## Security

Every target, bundle, captured URL, verification request, and probe is subject
to the shared URL/SSRF boundary. The production client validates resolved
addresses at connect time through `SsrfBlockingResolver`.

Verification and RPC probing are unauthenticated by design. They never attach
Axon credentials. Chrome capture executes page code and requires an explicitly
configured Chrome endpoint.

See [Security](../operations/security.md).

## Tests

- service tests: `crates/axon-services/src/endpoints_tests.rs`
- candidate tests: `crates/axon-services/src/endpoints/candidates_tests.rs`
- probe tests: `crates/axon-services/src/endpoints/probe_tests.rs`
- core extraction tests: `crates/axon-core/src/content/endpoints_tests.rs`
- transport tests: matching CLI, MCP, and web crate tests

## Related documentation

- [Action reference](actions/endpoints.md)
- [HTTP API](http-api.md)
- [MCP tool contract](mcp/tool-contract.md)
- [Configuration](../guides/configuration.md)
