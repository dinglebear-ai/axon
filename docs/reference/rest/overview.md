# REST Overview
Last Modified: 2026-07-15

The REST surface is a transport over `axon-api` DTOs and `axon-services`.

## Rules

- REST routes do not own alternate pipeline behavior.
- OpenAPI must be generated from the live route and DTO surface.
- Removed routes must return not found or an equivalent non-dispatch result.
- Source acquisition uses the canonical source route.

## Generated Files

OpenAPI JSON, markdown, and schema references live under
`docs/reference/rest/`.
