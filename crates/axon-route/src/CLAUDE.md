# axon-route — Agent Guide

`axon-route` owns **source resolution and routing** — every source target passes
through here before acquisition. It resolves inputs to a canonical URI + source
id, maps known authorities/aliases, validates scopes, and picks the adapter by
capability. It answers "what canonical source is this, and which adapter (at what
scope) may acquire it." Full contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-route/README.md](../../../docs/pipeline-unification/crates/axon-route/README.md)
· behavior spec:
[../../../docs/pipeline-unification/sources/url-normalization.md](../../../docs/pipeline-unification/sources/url-normalization.md)
· [../../../docs/pipeline-unification/sources/adapter-scopes.md](../../../docs/pipeline-unification/sources/adapter-scopes.md).

## Status — live crate, Phase 4 landed
URL canonicalization, capability detection, and source resolution/routing
(`canonical.rs`/`capability.rs`/`resolver.rs`/`router.rs`) are real and tested,
not markers. Do not add acquisition/fetching, parsing, ledger, or vector
behavior here.

## Module map
| File | Owns |
|---|---|
| `resolver.rs` | `SourceResolver` — input → `ResolvedSource` |
| `router.rs` | `SourceRouter` — `ResolvedSource` → `RouteDecision` + `AdapterMatch` |
| `canonical.rs` | `CanonicalUri` construction/normalization |
| `source_id.rs` | `SourceId` — deterministic identity |
| `scope.rs` | `SourceScope` — scope declaration + validation |
| `authority.rs` | `AuthorityRecord` — official docs/repo/package/registry mapping |
| `alias.rs` | `AliasRecord` — alias → authority resolution (no network) |
| `capability.rs` | adapter capability matching |
| `testing.rs` | alias/GitHub/local/web-scope + denied-scope fixtures |

## Boundary — keep OUT of this crate
- Fetching/acquiring content, HTTP clients for source bodies.
- Parsing, chunking, embedding, vector writes, graph persistence.
- Transport command parsing.
- Provider credentials beyond route-time auth requirements.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-authz`, URL/domain parsing crates.
- **Forbidden:** `axon-adapters` internals, fetching HTTP clients, `axon-ledger`/`axon-parse`/`axon-vectors`/`axon-services`, transport crates. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- **Same input → same canonical URI + source id** — resolution is deterministic.
- **Aliases resolve without network access.**
- **Ambiguous scopes produce actionable errors** — never a silent guess.
- **Adapter selection is deterministic** when multiple adapters match, via declared capability.
- Every source request passes through route **before** acquisition; scopes are declared and validated before execution.
- Web `SourceRequest` inputs normalize from bare domains and known aliases; `map` stays a first-class route.

## DTO ownership
Wire DTOs (`ResolvedSource`, `RouteDecision`, `CanonicalUri`, `SourceId`,
`SourceScope`, `AuthorityRecord`, `AliasRecord`, `AdapterMatch`) are defined in
**`axon-api`**; this crate constructs and returns them — it does not redefine
transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `sources/url-normalization.md` ·
`sources/adapter-scopes.md` · route/scope schemas in `schemas/api-dto-schema.md` ·
the route DTO components in `axon-api`.
