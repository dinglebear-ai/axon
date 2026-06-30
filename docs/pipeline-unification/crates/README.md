# Crate Contracts
Last Modified: 2026-06-30

## Contract

This directory owns per-crate implementation contracts for the pipeline
unification target crate set.

`foundation/crate-structure.md` is the high-level dependency and ownership map.
Each `crates/<crate>/README.md` here is the human-facing implementation
contract for the target crate: purpose, public modules, owned
traits/DTOs/services, dependencies, generated artifacts, fixtures, tests, and
acceptance criteria.

When the real crate exists, `crates/<crate>/src/CLAUDE.md` is the agent
source-of-truth maintenance contract. The README must be generated from or
checked against `src/CLAUDE.md` plus rustdoc so human docs and agent docs cannot
drift.

These docs are intentionally under `docs/pipeline-unification/crates/` to avoid
confusion with the real repo `crates/` source directory.

## Crate Index

| Crate | Contract |
|---|---|
| `axon-error` | [axon-error/README.md](axon-error/README.md) |
| `axon-api` | [axon-api/README.md](axon-api/README.md) |
| `axon-authz` | [axon-authz/README.md](axon-authz/README.md) |
| `axon-core` | [axon-core/README.md](axon-core/README.md) |
| `axon-observe` | [axon-observe/README.md](axon-observe/README.md) |
| `axon-route` | [axon-route/README.md](axon-route/README.md) |
| `axon-adapters` | [axon-adapters/README.md](axon-adapters/README.md) |
| `axon-ledger` | [axon-ledger/README.md](axon-ledger/README.md) |
| `axon-parse` | [axon-parse/README.md](axon-parse/README.md) |
| `axon-graph` | [axon-graph/README.md](axon-graph/README.md) |
| `axon-memory` | [axon-memory/README.md](axon-memory/README.md) |
| `axon-document` | [axon-document/README.md](axon-document/README.md) |
| `axon-embedding` | [axon-embedding/README.md](axon-embedding/README.md) |
| `axon-vectors` | [axon-vectors/README.md](axon-vectors/README.md) |
| `axon-retrieval` | [axon-retrieval/README.md](axon-retrieval/README.md) |
| `axon-llm` | [axon-llm/README.md](axon-llm/README.md) |
| `axon-prune` | [axon-prune/README.md](axon-prune/README.md) |
| `axon-jobs` | [axon-jobs/README.md](axon-jobs/README.md) |
| `axon-services` | [axon-services/README.md](axon-services/README.md) |
| `axon-mcp` | [axon-mcp/README.md](axon-mcp/README.md) |
| `axon-web` | [axon-web/README.md](axon-web/README.md) |
| `axon-cli` | [axon-cli/README.md](axon-cli/README.md) |

## Standard Crate Contract Sections

Every crate README must include:

- purpose
- owns
- must not own
- public modules
- public API
- dependencies allowed
- dependencies forbidden
- generated artifacts
- fixtures and fakes
- tests
- acceptance criteria

## Dependency Layer Rule

Crates must follow the dependency direction in
[../foundation/crate-structure.md](../foundation/crate-structure.md). Lower
layers never depend on higher orchestration or transport layers.

Transport crates (`axon-cli`, `axon-mcp`, `axon-web`) must consume
`axon-services` and `axon-api`; they must not import domain crate internals.

## Public Type Ownership

Serializable wire DTOs, enum registries, request/result envelopes, pagination,
error projections, progress projections, and capability documents are defined in
`axon-api`.

Domain crates may define internal helper structs and builder types, but any type
that crosses CLI, MCP, REST, jobs, generated schemas, or app boundaries must
either be:

- defined in `axon-api`, or
- explicitly documented as a domain-local type that is converted to an
  `axon-api` DTO before crossing a boundary.

Per-crate READMEs must label public API entries as `defined here`,
`re-exported from axon-api`, or `internal builder`. This prevents accidental
second DTO definitions.

## Acceptance Criteria

- every target crate has a per-crate contract README
- every crate contract names owned and forbidden responsibilities
- every crate contract names required public modules and tests
- every crate contract links back to the high-level crate structure contract
- crate contracts do not conflict with `foundation/crate-structure.md`
- every real crate `src/CLAUDE.md` has sibling `AGENTS.md` and `GEMINI.md`
  symlinks pointing to it
