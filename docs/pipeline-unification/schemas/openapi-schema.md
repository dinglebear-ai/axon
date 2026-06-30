# OpenAPI Schema Contract
Last Modified: 2026-06-30

## Contract

The REST/OpenAPI schema is generated from `axon-web` routes and `axon-api` DTOs.
It is the exact machine-readable REST contract. REST behavior lives in
[../surfaces/rest-contract.md](../surfaces/rest-contract.md); this file owns the
OpenAPI artifact shape and drift checks.

## Generated Artifacts

```text
docs/reference/rest/openapi.json
docs/reference/rest/openapi.md
docs/reference/rest/schemas.md
```

Generator:

```bash
cargo xtask schemas openapi
cargo xtask schemas openapi --check
```

## Source Inputs

The OpenAPI generator reads:

```text
crates/axon-web/src/routes/**
crates/axon-web/src/route_registry.rs
crates/axon-web/src/auth*.rs
crates/axon-api/src/**
crates/axon-error/src/**
crates/axon-observe/src/**
docs/pipeline-unification/surfaces/rest-contract.md
```

The generated OpenAPI document records these paths in `x-axon.source_inputs`.

## Required OpenAPI Shape

- OpenAPI version: `3.1.0`
- `info.title`: `Axon API`
- `info.version`: workspace package version
- every `/v1/*` route from `rest-contract.md` is present
- removed routes are absent
- auth requirements are represented per route
- request/response bodies reference `axon-api` component schemas
- error responses use the `axon-error` projection schema
- streaming routes document SSE event schema from `axon-observe`

## Root Document Shape

```json
{
  "openapi": "3.1.0",
  "info": {
    "title": "Axon API",
    "version": "<workspace-version>"
  },
  "x-axon": {
    "contract_version": "2026-06-30",
    "generated_by": "cargo xtask schemas openapi",
    "owner_crates": ["axon-web", "axon-api"],
    "clean_break": true
  },
  "paths": {},
  "components": {
    "schemas": {},
    "securitySchemes": {}
  },
  "tags": []
}
```

## Route Object Requirements

Every route operation includes:

- `operationId` in `snake_case`
- `summary`
- `tags`
- `security`
- `parameters`
- `requestBody` when applicable
- `responses.200` or route-specific success code
- `responses.400`
- `responses.401` when protected
- `responses.403` when scoped
- `responses.500`
- `x-axon.action` or `x-axon.service`
- `x-axon.required_scope`
- `x-axon.mutates`
- `x-axon.async`

Example route metadata:

```json
{
  "x-axon": {
    "service": "SourceService.submit",
    "request_dto": "SourceRequest",
    "result_dto": "SourceResult",
    "required_scope": "axon:write",
    "mutates": true,
    "async": true
  }
}
```

## Complete Route Families

The schema must include every route in the End-State Route Inventory from
`rest-contract.md`. Route families are grouped only for readability; there is no
partial/minimum OpenAPI surface.

| Family | Required Paths |
|---|---|
| Health/status | `/healthz`, `/readyz`, `/metrics`, `/v1/server`, `/v1/status`, `/v1/doctor`, `/v1/stats` |
| Sources | `/v1/resolve`, `/v1/sources`, `/v1/sources/{source_id}`, `/v1/sources/{source_id}/refresh`, `/v1/sources/{source_id}/items`, `/v1/sources/{source_id}/documents`, `/v1/sources/{source_id}/generations`, `/v1/domains` |
| Documents | `/v1/documents/{document_id}`, `/v1/documents/{document_id}/chunks`, `/v1/documents/{document_id}/chunks/{chunk_id}` |
| Jobs | `/v1/jobs`, `/v1/jobs/{job_id}`, `/v1/jobs/{job_id}/events`, `/v1/jobs/{job_id}/stream`, `/v1/jobs/{job_id}/artifacts`, `/v1/jobs/recover`, `/v1/jobs/cleanup` |
| Watches | `/v1/watches`, `/v1/watches/{watch_id}`, `/v1/watches/{watch_id}/exec`, `/v1/watches/{watch_id}/pause`, `/v1/watches/{watch_id}/resume`, `/v1/watches/{watch_id}/history` |
| Retrieval/synthesis | `/v1/search`, `/v1/query`, `/v1/retrieve`, `/v1/ask`, `/v1/ask/stream`, `/v1/chat`, `/v1/chat/stream`, `/v1/evaluate`, `/v1/suggest`, `/v1/research`, `/v1/research/stream`, `/v1/summarize`, `/v1/summarize/stream` |
| Analysis/inspection | `/v1/map`, `/v1/endpoints`, `/v1/brand`, `/v1/diff`, `/v1/screenshot`, `/v1/extract` |
| Graph/memory | `/v1/graph/*`, `/v1/memories/*` |
| Operations | `/v1/prune/*`, `/v1/collections`, `/v1/providers`, `/v1/adapters`, `/v1/artifacts`, `/v1/uploads` |
| Panel/mobile | `/api/panel/*`, `/v1/mobile/sessions*` |

## Complete Route Inventory Contract

The generator reads the canonical REST route registry from `axon-web`. The
registry record for every route must contain:

```rust
pub struct RestRouteSpec {
    pub method: HttpMethod,
    pub path: &'static str,
    pub operation_id: &'static str,
    pub tag: &'static str,
    pub summary: &'static str,
    pub request_dto: Option<&'static str>,
    pub result_dto: &'static str,
    pub service: &'static str,
    pub required_scope: Option<AuthScope>,
    pub mutates: bool,
    pub async_job: bool,
    pub streaming: bool,
}
```

The OpenAPI generator must fail if a route handler is not registered with a
`RestRouteSpec`.

Canonical route registry examples:

| Method | Path | Operation ID | Request DTO | Result DTO | Scope |
|---|---|---|---|---|---|
| `GET` | `/healthz` | `healthz` | none | `HealthReport` | none |
| `GET` | `/readyz` | `readyz` | none | `HealthReport` | none |
| `GET` | `/v1/server` | `get_server` | none | `ServerInfo` | `axon:read` |
| `GET` | `/v1/capabilities` | `get_capabilities` | none | `CapabilityDocument` | `axon:read` |
| `POST` | `/v1/resolve` | `resolve_source` | `ResolveSourceRequest` | `ResolvedSource` | `axon:read` |
| `POST` | `/v1/sources` | `submit_source` | `SourceRequest` | `SourceResult` | `axon:write` |
| `GET` | `/v1/sources` | `list_sources` | `SourceListRequest` | `Page<SourceSummary>` | `axon:read` |
| `GET` | `/v1/sources/{source_id}` | `get_source` | none | `SourceSummary` | `axon:read` |
| `GET` | `/v1/sources/{source_id}/items` | `list_source_items` | `SourceItemListRequest` | `Page<SourceItem>` | `axon:read` |
| `GET` | `/v1/sources/{source_id}/generations` | `list_source_generations` | `SourceGenerationListRequest` | `Page<SourceGenerationSummary>` | `axon:read` |
| `GET` | `/v1/documents` | `list_documents` | `DocumentListRequest` | `Page<DocumentSummary>` | `axon:read` |
| `GET` | `/v1/documents/{document_id}` | `get_document` | none | `DocumentDetail` | `axon:read` |
| `GET` | `/v1/documents/{document_id}/chunks` | `list_document_chunks` | `ChunkListRequest` | `Page<ChunkSummary>` | `axon:read` |
| `GET` | `/v1/jobs` | `list_jobs` | `JobListRequest` | `Page<JobSummary>` | `axon:read` |
| `GET` | `/v1/jobs/{job_id}` | `get_job` | none | `JobSummary` | `axon:read` |
| `GET` | `/v1/jobs/{job_id}/events` | `list_job_events` | `JobEventListRequest` | `JobEventPage` | `axon:read` |
| `GET` | `/v1/jobs/{job_id}/stream` | `stream_job_events` | none | `SourceProgressEvent` SSE | `axon:read` |
| `POST` | `/v1/jobs/{job_id}/cancel` | `cancel_job` | none | `JobSummary` | `axon:write` |
| `POST` | `/v1/jobs/{job_id}/retry` | `retry_job` | none | `JobDescriptor` | `axon:write` |
| `POST` | `/v1/search` | `search` | `SearchRequest` | `SearchResult` | `axon:read` |
| `POST` | `/v1/query` | `query` | `QueryRequest` | `QueryResult` | `axon:read` |
| `POST` | `/v1/retrieve` | `retrieve` | `RetrievalRequest` | `RetrievalResult` | `axon:read` |
| `POST` | `/v1/ask` | `ask` | `AskRequest` | `AskResult` | `axon:read` |
| `POST` | `/v1/extract` | `extract` | `ExtractRequest` | `ExtractResult` | `axon:write` |
| `POST` | `/v1/prune/plan` | `plan_prune` | `PruneRequest` | `PrunePlan` | `axon:admin` |
| `POST` | `/v1/prune/exec` | `execute_prune` | `PruneExecuteRequest` | `PruneResult` | `axon:admin` |

This table is illustrative, not a separate source of truth. The generated
OpenAPI check must compare the `axon-web` route registry against
`rest-contract.md` and fail unless every end-state route is present with matching
method, path, auth scope, request DTO, result DTO, mutability, and streaming
metadata.

## Parameter Rules

- path parameters are required and typed by shared id schemas
- query parameters are only for list/filter/pagination controls
- complex input uses JSON request body
- headers used by auth are documented in security schemes, not per-route custom
  prose
- SSE routes use `text/event-stream` and event schema

## Response Rules

Every non-streaming success response is:

```json
{
  "$ref": "#/components/schemas/SuccessEnvelope_<ResultDto>"
}
```

Every domain error response is:

```json
{
  "$ref": "#/components/schemas/ErrorEnvelope"
}
```

Status code requirements:

| Status | Meaning |
|---|---|
| `200` | synchronous success |
| `202` | accepted async job |
| `400` | validation error |
| `401` | missing/invalid auth |
| `403` | insufficient scope |
| `404` | known id not found |
| `409` | lease/conflict/idempotency conflict |
| `429` | rate limit/cooling |
| `500` | internal failure |
| `503` | required provider unavailable |

## Component Families

Required components:

- `SourceRequest`, `SourceResult`, `ResolvedSource`
- `JobDescriptor`, `JobSummary`, `JobEventPage`
- `SourceProgressEvent`
- `ApiError`, `ErrorEnvelope`, `SuccessEnvelope`
- `QueryRequest`, `QueryResult`
- `RetrievalRequest`, `RetrievalResult`
- `AskRequest`, `AskResult`
- `Memory*`
- `Graph*`
- `Prune*`
- `ProviderCapability`
- `CollectionSpec`

## Drift Checks

Fail when:

- REST route exists in code but not OpenAPI
- OpenAPI route exists without handler
- OpenAPI route is missing auth metadata
- OpenAPI schema differs from `axon-api`
- removed route appears
- examples in `rest-contract.md` fail validation

## Acceptance Tests

- generated OpenAPI validates with an OpenAPI 3.1 validator
- every handler has an operation id
- every route example in `rest-contract.md` validates against request schema
- every documented error response uses `ErrorEnvelope`
- no removed routes appear in `paths`

## Validation Fixtures

Required fixtures:

```text
crates/axon-web/tests/fixtures/openapi/source_request.valid.json
crates/axon-web/tests/fixtures/openapi/query_request.valid.json
crates/axon-web/tests/fixtures/openapi/error_envelope.valid.json
crates/axon-web/tests/fixtures/openapi/removed_route_absent.snapshot
```

## Acceptance Criteria

- every REST handler has a `RestRouteSpec`
- every `RestRouteSpec` appears in OpenAPI
- every OpenAPI route has a handler
- route auth metadata matches `auth-contract.md`
- OpenAPI components reuse `axon-api` schemas
- SSE routes use `SourceProgressEvent`
