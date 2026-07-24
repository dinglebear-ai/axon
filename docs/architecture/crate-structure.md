# Crate Structure

Last Modified: 2026-07-19

Axon is a Cargo workspace: a thin root `axon` binary that delegates to
`axon-cli`, plus 23 focused crates under `crates/`. All crates inherit the
product version via `version.workspace = true` (currently 7.1.5, edition 2024,
rust-version 1.94.0).

> The contract target for this layout lives at
> [`docs/pipeline-unification/foundation/crate-structure.md`](../pipeline-unification/foundation/crate-structure.md).
> This document describes the **current** workspace; the dependency-direction
> rules are enforced by `cargo xtask check-layering` (see
> [dependency-layering.md](dependency-layering.md)).

## Layering

```text
axon-error  ──┐
              ├→ axon-api ──┬→ axon-core ──┐
              │              ├→ axon-authz  │
              │              └→ axon-observe┤
              │                             │
   axon-route ──────────────────────────────┤
   axon-parse                               │
   axon-adapters ─→ axon-extract            │
   axon-ledger   ─→ axon-graph              ├→ axon-document
   axon-memory                              │
   axon-embedding ─→ axon-vectors ─→ axon-retrieval
   axon-llm                                 │
   axon-prune                               │
              ↓                             │
           axon-jobs ───────────────────────┘
              ↓
           axon-services  (facade: composition + job runtime)
              ↓
   axon-cli   axon-mcp   axon-web   (transports)
              ↓
            axon (root binary)
```

Direction flows downward. Transport crates (cli/mcp/web) never reach into a
domain crate's internal `::ops::*` modules — they call `axon-services` or a
domain crate's public surface. `axon-services` is a facade, not a mandatory
reimplementation hop: single-domain logic stays in its owning crate.

## Workspace members

Listed in dependency-layer order (bottom to top). "Deps" shows only axon-internal
edges, read from each crate's `Cargo.toml`.

### Cross-cutting (leaf contracts and shared infra)

| Crate | Purpose | Axon deps |
|---|---|---|
| `axon-error` | typed error taxonomy: codes, stage/severity/retry/degradation/cooling | (leaf) |
| `axon-api` | transport-neutral DTO/enum/envelope/schema hub (`SourceRequest`, `SourceResult`, job DTOs, MCP schema) | `axon-error` |
| `axon-authz` | auth scopes, policy decisions, execution visibility, credential resolution | `axon-api`, `axon-error` |
| `axon-core` | config, paths, redaction, HTTP safety, time/id, artifact primitives | `axon-api` |
| `axon-observe` | progress events, spans, metrics, structured logs, heartbeats | `axon-api`, `axon-core`, `axon-error` |

### Domain (acquisition, preparation, storage, retrieval)

| Crate | Purpose | Axon deps |
|---|---|---|
| `axon-route` | source resolution, canonical URI, adapter/scope routing | `axon-api`, `axon-error` |
| `axon-parse` | parsers: code (tree-sitter), markdown, manifest, schema, session, tool | `axon-api` |
| `axon-extract` | structured LLM extraction (vertical extractors) | `axon-core`, `axon-llm`, `axon-api` |
| `axon-adapters` | per-source-family acquisition (web, local, git, registry, reddit, youtube, feed, sessions, cli/mcp tools) | `axon-api`, `axon-core`, `axon-error`, `axon-extract`, `axon-parse`, `axon-observe` |
| `axon-ledger` | SQLite source ledger: sources, items, manifests, diffs, generations, leases, cleanup debt | `axon-api`, `axon-error` |
| `axon-graph` | GraphStore trait + SQLite impl: nodes, edges, evidence, merge/conflict | `axon-api`, `axon-core`, `axon-error` |
| `axon-memory` | MemoryStore + decay/reinforcement/review/forgetting | `axon-api`, `axon-core`, `axon-document`, `axon-embedding`, `axon-error`, `axon-graph`, `axon-observe`, `axon-vectors` |
| `axon-document` | DocumentPreparer, ChunkRouter, PreparedDocument | `axon-api`, `axon-core`, `axon-parse` |
| `axon-embedding` | EmbeddingProvider trait + TEI / OpenAI-compat / fake | `axon-api`, `axon-error`, `axon-observe` |
| `axon-vectors` | VectorStore trait + Qdrant impl (named dense + BM42 sparse) | `axon-api`, `axon-core`, `axon-error`, `axon-observe` |
| `axon-retrieval` | RetrievalEngine: query/retrieve/ask context, ranking/fusion | `axon-api`, `axon-embedding`, `axon-error`, `axon-vectors` |
| `axon-llm` | LlmProvider trait + Gemini-headless / OpenAI-compat / Codex app-server / fake | `axon-api`, `axon-core`, `axon-error`, `axon-observe` |
| `axon-prune` | cleanup, purge, dedupe, cleanup-debt executor | `axon-api` |

### Composition + runtime

| Crate | Purpose | Axon deps |
|---|---|---|
| `axon-jobs` | JobStore, workers, events, scheduler, heartbeats, watch store | `axon-api`, `axon-adapters`, `axon-core`, `axon-error`, `axon-graph`, `axon-ledger`, `axon-llm`, `axon-memory`, `axon-observe` |
| `axon-services` | orchestration facade + ServiceContext (the source runner keeps one job id across all stages) | all 17 lower crates |

### Transports (thin projections over `axon-services`)

| Crate | Purpose | Axon deps |
|---|---|---|
| `axon-mcp` | MCP transport: single `axon` tool, `action`/`subaction` routing | `axon-api`, `axon-authz`, `axon-core`, `axon-services` |
| `axon-web` | REST/OpenAPI/panel transport (Axum) | `axon-api`, `axon-authz`, `axon-core`, `axon-error`, `axon-jobs`, `axon-llm`, `axon-services` |
| `axon-cli` | CLI transport (clap parser, rendering) | `axon-adapters`, `axon-api`, `axon-core`, `axon-jobs`, `axon-mcp`, `axon-services`, `axon-web` |

### Binary

| Crate | Purpose | Axon deps |
|---|---|---|
| `axon` (root `.`) | binary bootstrap (`src/main.rs` + `src/lib.rs` re-export `axon_cli::run`) | `axon-cli`, `axon-core`, `axon-mcp`, `axon-services`, `axon-web` |

## Ownership rule

Own the contract where the data lives:

- **Single-domain logic** → its domain crate (e.g. prune plan/execute in `axon-prune`).
- **The `*Result` DTO** → `axon-api` (e.g. `PurgeResult`).
- **Cross-domain composition or job-runtime wiring** → `axon-services` (thin
  facade, often just a re-export).
- **Transports** never import a domain crate's internal `::ops::*` modules.

Authoritative doc: [crate-ownership.md](crate-ownership.md). Enforced by
`cargo xtask check-layering`.

## Per-crate maintenance contracts

Every non-trivial crate has `crates/<name>/src/CLAUDE.md` — the agent-facing
maintenance contract (purpose, public modules, ownership boundaries,
must-not-own, test commands, gotchas). `AGENTS.md` and `GEMINI.md` in the same
dir are symlinks to it. The pipeline-unification packet also keeps design
contracts at `docs/pipeline-unification/crates/<name>/{README.md,CLAUDE.md}`
(historical design record; the live `src/CLAUDE.md` supersedes).

## Notes on transitional state

- The legacy single-purpose crates (`axon-vector`, `axon-crawl`, `axon-ingest`,
  `axon-code-index`) are removed from the workspace. `cargo xtask check-layering`
  still carries forbidden-module prefixes and a small allowlist referencing
  their old paths as a guardrail — see [dependency-layering.md](dependency-layering.md).
- `axon-extract` remains even though the contract disposition said "remove or
  shrink"; it is still depended on by `axon-adapters` and `axon-services` for
  vertical extraction.

If this layout changes, update this file in the same PR.
