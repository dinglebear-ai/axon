---
title: "Crate Structure"
created: 2026-07-15
updated: 2026-09-29
---

# Crate Structure

Last reviewed: 2026-09-29

Axon is a Cargo workspace with a thin root binary, focused product crates,
and repository-maintenance packages. [`Cargo.toml`](../../Cargo.toml) defines
membership and inherited product metadata. The Palette Tauri backend has a
separate workspace; it is not a root-workspace crate.

This page describes responsibilities. Exact dependency edges belong to the
[generated dependency graph](../reference/crate-dependency-graph.md), not a
second hand-maintained dependency table. The [public API snapshot](../reference/public-api-surface.md)
records exported surfaces. Regenerate these when their source changes.

## Layering

```text
Contracts and shared policy
        ↓
Domain acquisition / preparation / storage / retrieval
        ↓
Durable execution and cross-domain service composition
        ↓
CLI / MCP / HTTP projections
        ↓
Root binary and client applications
```

This is an ownership sketch, not a complete Cargo graph. Dependency-direction
rules, provider boundaries, and exact reviewed exceptions are described in
[dependency-layering.md](dependency-layering.md) and enforced by
`cargo xtask check-layering`. Transports call shared services/public contracts,
not domain-internal `::ops::*`.

## Workspace members

### Contracts and shared infrastructure

| Crate | Responsibility |
|---|---|
| `axon-error` | Typed error taxonomy, cause, stage, severity, retry, and degradation contracts |
| `axon-api` | Transport-neutral DTOs, enums, envelopes, and schema contracts |
| `axon-authz` | Authorization scopes, policy decisions, and execution visibility |
| `axon-core` | Configuration, paths, HTTP safety, redaction, and common primitives |
| `axon-observe` | Progress events, tracing, metrics, and correlated diagnostics |

### Acquisition, preparation, storage, and retrieval

| Crate | Responsibility |
|---|---|
| `axon-route` | Canonical source identity, adapter selection, scope/options routing |
| `axon-adapters` | Source-owned materialization, discovery, acquisition, and normalization |
| `axon-extract` | Structured/vertical extraction capabilities used by acquisition/services |
| `axon-parse` | Parser implementations and source facts for text, code, manifests, and structured inputs |
| `axon-document` | Document preparation and chunk construction |
| `axon-ledger` | Source generations, manifests, items, document status, leases, and cleanup debt |
| `axon-embedding` | Embedding providers and their retry/cooling behavior |
| `axon-vectors` | Vector storage, payloads, upserts, and publication |
| `axon-retrieval` | Filtered retrieval, ranking/fusion, and context retrieval |
| `axon-graph` | Graph storage, nodes/edges, evidence, and publication coordination |
| `axon-memory` | Memory retention, retrieval, review, decay, and forgetting |
| `axon-llm` | Synthesis provider implementations and request handling |
| `axon-codex` | Typed Codex app-server control, bounded events/approvals, and mutation tracking |
| `axon-prune` | Cleanup planning/execution over source and derived state |

### Execution, composition, and transports

| Crate | Responsibility |
|---|---|
| `axon-jobs` | Durable SQLite jobs, attempts, stages, events, heartbeats, reservations, and worker runtime |
| `axon-services` | Cross-domain orchestration, policy/context binding, and transport-facing service entry points |
| `axon-cli` | CLI parsing/dispatch, progress, and output rendering |
| `axon-mcp` | Legacy and atomic tool projections, resources, auxiliary dashboard tool, and protocol tasks |
| `axon-web` | REST/OpenAPI, HTTP MCP mounting, web-panel assets, and HTTP authorization integration |
| `axon` | Root binary bootstrap and delegation |

`xtask` and `xtask-release` maintain checks, generated contracts, and release
operations. They are tooling packages, not additional source-pipeline domains,
and their package versions need not equal the product version.

## Ownership rule

Shared DTOs belong to `axon-api`; single-domain logic belongs to its owning
crate; cross-domain or job-aware composition belongs to `axon-services`.
Adapters emit `SourceDocument` values rather than running private embedding
or publication pipelines. See [crate ownership](crate-ownership.md) and
[source pipeline](source-pipeline.md).

## Per-crate maintenance contracts

The canonical agent-facing maintenance file is
`crates/<name>/src/AGENTS.md` where that scope has local instructions.
`CLAUDE.md` and `GEMINI.md` are direct relative symlinks to `AGENTS.md`, not
the reverse. Edit the canonical file and preserve the aliases.

The pipeline-unification packet retains per-crate design contracts under
`docs/pipeline-unification/crates/`. Treat dated implementation plans as
history; reconcile current ownership with code and the live instruction chain.
See [documentation maintenance](../development/documentation.md).

## Notes on transitional state

The old `axon-vector`, `axon-crawl`, `axon-ingest`, and `axon-code-index` crates
are not workspace members. Acquisition now lives in adapters, with separate
preparation, embedding, publication, and retrieval owners. `axon-extract`
remains an active crate; a historical plan to shrink/remove it is not proof
that it was removed.

Use `cargo xtask check-layering` for enforced boundaries and
`cargo xtask check-crate-contracts` for the standalone design-contract audit.
The latter is not a substitute for testing runtime behavior. Run the ordered
`cargo xtask generated-contracts refresh` and `check` after changing sources
that own generated references.
