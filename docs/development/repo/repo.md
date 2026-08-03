---
title: "Repository Structure"
updated: 2026-08-02
---

# Repository Structure

Axon is a Cargo workspace with a thin root binary, 23 focused Rust crates,
client applications, deployment assets, generated references, and repository
tooling.

## Top-level tree

```text
axon/
├── src/                     thin binary/library shim and integration binaries
├── crates/                  23 Rust workspace crates
├── apps/                    web, Android, Chrome extension, Palette desktop
├── deploy/                  Incus and native systemd deployment assets
├── docs/                    living docs plus dated history directories
├── migrations/              root migrations retained by the product
├── plugins/axon/            Claude plugin, commands, agents, and skills
├── scripts/                 repository and operational automation
├── tests/                   root integration tests and fixtures
├── xtask/                   generators and repository contract checks
├── config/                  Chrome, Qdrant, container, and MCP support files
├── vendor/                  patched local dependencies
├── Cargo.toml               workspace manifest and product metadata
├── Cargo.lock               locked Rust dependency graph
├── Justfile                 developer and verification recipes
├── .env.example             endpoint, credential, and bootstrap template
├── config.example.toml      non-secret runtime tuning template
└── CLAUDE.md                root agent/repository instructions
```

The authoritative architecture view is
[Repository Structure](../../architecture/repo-structure.md). This page focuses
on contributor navigation.

## Rust workspace layers

### Cross-cutting contracts

- `axon-error`: typed error taxonomy
- `axon-api`: transport-neutral DTOs, enums, envelopes, schemas
- `axon-authz`: caller, scope, visibility, HTTP auth policy
- `axon-core`: configuration, paths, HTTP safety, content, redaction
- `axon-observe`: events, progress, metrics, spans, logging

### Domain crates

- `axon-route`: source classification and routing
- `axon-adapters`: source-family acquisition and web engine
- `axon-document`: normalization, parsing bridge, chunk preparation
- `axon-parse`: parser registry and parse facts
- `axon-extract`: vertical structured extraction
- `axon-ledger`: source manifests, generations, leases, cleanup debt
- `axon-graph`: graph nodes, edges, evidence, persistence
- `axon-embedding`: embedding provider boundary
- `axon-vectors`: vector store and Qdrant implementation
- `axon-retrieval`: query/retrieve/context/ranking
- `axon-llm`: synthesis provider boundary
- `axon-memory`: durable memory lifecycle
- `axon-prune`: cleanup planning and execution

### Runtime and composition

- `axon-jobs`: durable jobs, scheduler, workers, watches, recovery
- `axon-services`: cross-domain orchestration and runtime composition

### Transports

- `axon-cli`: command parser and terminal rendering
- `axon-mcp`: one MCP tool with action/subaction routing
- `axon-web`: REST, OpenAPI, MCP-over-HTTP, and bundled panel server

The root `axon` package is a thin bootstrap over these crates.

## Where to make changes

| Change | Primary location |
|---|---|
| Source classification or scope | `crates/axon-route/src/` |
| Acquisition behavior | `crates/axon-adapters/src/` |
| Chunking or preparation | `crates/axon-document/src/` |
| Job lifecycle or scheduler | `crates/axon-jobs/src/` |
| Cross-domain orchestration | `crates/axon-services/src/` |
| CLI parsing/rendering | `crates/axon-cli/src/` |
| MCP request/handler | `crates/axon-api/src/mcp_schema/`, `crates/axon-mcp/src/` |
| REST/OpenAPI route | `crates/axon-web/src/server/` |
| Generated schema/docs | `xtask/src/schemas/`, `xtask/src/docs/` |
| Web UI | `apps/web/` |
| Mobile/desktop/extension | matching `apps/<component>/` directory |

## Module layout

Rust uses file-per-module layout. `mod.rs` is forbidden:

```text
foo.rs
foo/
  bar.rs
  baz.rs
```

Enforcement: `cargo xtask check-no-mod-rs`.

## Per-crate instructions

Each non-trivial crate has `crates/<name>/src/CLAUDE.md` with its ownership,
public surface, test commands, and gotchas. `AGENTS.md` and `GEMINI.md` are
symlinks to the same contract.

## Verification

```bash
cargo fmt --all -- --check
cargo xtask check-layering
cargo xtask check-crate-contracts
cargo xtask check-repo-structure
cargo xtask docs check
just precommit
```

See [Rules](rules.md), [Recipes](recipes.md), and
[Testing](../testing.md) for the complete contributor workflow.
