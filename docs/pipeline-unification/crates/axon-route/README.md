# axon-route Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-route` owns source resolution, URI normalization, adapter routing, scope
selection, source identity, aliases, and authority records.

## Owns

- `SourceResolver` and `SourceRouter`
- canonical URI and source id construction
- authority mapping for known official docs, repos, packages, registries, and
  aliases
- adapter capability matching and scope validation
- source graph seed links discovered during resolution

## Must Not Own

- fetching/acquiring content
- parsing, chunking, embedding, vector writes, or graph persistence
- transport command parsing
- provider-specific credentials beyond route-time auth requirements

## Public Modules

```text
lib.rs
resolver.rs
router.rs
canonical.rs
source_id.rs
scope.rs
authority.rs
alias.rs
capability.rs
testing.rs
```

## Public API

- `SourceResolver`
- `SourceRouter`
- `ResolvedSource`
- `RoutePlan`
- `RouteDecision`
- `CanonicalUri`
- `SourceId`
- `SourceScope`
- `AuthorityRecord`
- `AliasRecord`
- `AdapterDefinition`
- `AdapterRegistry`
- `AdapterMatch`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-authz`
- URL/domain parsing crates

## Dependencies Forbidden

- `axon-adapters` implementation internals
- HTTP clients used for fetching source bodies
- `axon-ledger`, `axon-parse`, `axon-vectors`, `axon-services`
- transport crates

## Generated Artifacts

- route and scope schemas in [../../schemas/api-dto-schema.md](../../schemas/api-dto-schema.md)
- URL normalization examples in [../../sources/url-normalization.md](../../sources/url-normalization.md)

## Fixtures And Fakes

- shadcn-style alias to official docs/repo/package fixture
- GitHub repo URL and shorthand fixture
- local path fixture
- web page/site/repo scope fixture
- denied scope fixture

## Tests

- same input resolves to the same canonical URI and source id
- aliases resolve without network access
- ambiguous scopes produce actionable errors
- adapter selection is deterministic when multiple adapters match

## Acceptance Criteria

- every source request passes through route before acquisition
- adapter scopes are declared and validated before execution
- crawl entrypoints can be normalized from bare domains and known aliases

See [../README.md](../README.md) and
[../../sources/url-normalization.md](../../sources/url-normalization.md).
