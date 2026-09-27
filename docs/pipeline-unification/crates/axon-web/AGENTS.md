# axon-web Agent Instructions

This file is the agent-facing contract for the `axon-web` crate docs.

## When Editing

- Keep Axum routes, REST/OpenAPI mapping, SSE streams, web panel serving, HTTP
  auth integration, and route error mapping here.
- Do not bypass `axon-services` or import provider/store/domain internals.
- Do not add legacy route aliases.
- Update `README.md`, `../../surfaces/rest-contract.md`,
  `../../surfaces/web-contract.md`, and `../../schemas/openapi-schema.md`
  together.

## Review Checklist

- Routes use shared DTOs and envelopes.
- OpenAPI snapshots are deterministic.
- SSE events match `axon-observe` schemas.
