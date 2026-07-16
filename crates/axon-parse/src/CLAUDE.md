# axon-parse — Agent Guide

`axon-parse` owns **source parsing and fact extraction**: it turns
`SourceDocument` content into `SourceParseFacts` and `GraphCandidate` values that
downstream graph and chunking stages consume. It is the home of source-specific
intelligence — code AST facts, dependency manifests, schemas/OpenAPI, sessions,
tool calls, skills/agents, env examples, Docker Compose, and config — **before**
graph persistence or chunking. Full contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-parse/README.md](../../../docs/pipeline-unification/crates/axon-parse/README.md)
· behavior spec:
[../../../docs/pipeline-unification/sources/parsing-contract.md](../../../docs/pipeline-unification/sources/parsing-contract.md).

## Status — Phase 7 (wired)
The parser families below are implemented and now **consumed on the acquisition
path**: `axon-document`'s `parse.rs` bridge runs `builtins::production_registry()`
over each `SourceDocument`, so `SourceParseFacts`/`GraphCandidate` flow into
`PreparedDocument` and drive parser-aware chunk routing. Code symbol extraction
uses tree-sitter for Rust, Python, JavaScript, TypeScript, and TSX, with an
explicit regex fallback for unsupported or parse-failed input. Do not add
acquisition, graph persistence, chunking, or vector-write behavior here.

## Module map
| File | Owns |
|---|---|
| `parser.rs` | `SourceParser` trait + `ParserCapability` — parser capability declarations |
| `registry.rs` | `ParserRegistry` — language/filetype parser selection |
| `facts.rs` | `SourceParseFacts`, `ParseEvidence` — extracted facts with evidence spans |
| `graph_candidate.rs` | `GraphCandidate` — evidence-backed candidate edges/nodes for `axon-graph` |
| `code.rs` | `CodeSymbolFact` — AST-backed code symbol facts (tree-sitter) |
| `manifest.rs` | `DependencyFact` — Cargo/npm/Python dependency manifests |
| `schema.rs` | `ApiSchemaFact` — schemas + REST/OpenAPI specs |
| `session.rs` | `SessionFact` — AI session transcripts |
| `tool.rs` | tool-call / MCP tool schema, skills, agents parsers |
| `env.rs` / `docker.rs` / `config.rs` | env-example, Docker Compose, and config facts |
| `testing.rs` | `FakeParser` + parse fixtures (Cargo/npm/Python/compose/env/OpenAPI/JSONL/code) |

## Boundary — keep OUT of this crate
- Acquisition, ledger persistence, **graph persistence**, chunking output, vector writes, LLM extraction commands.
- Source routing or canonical source identity.
- IDE/LSP live query behavior.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-observe`; tree-sitter and format-specific parser crates behind parser modules; `axon-document` types only if needed.
- **Forbidden:** Qdrant/TEI/LLM clients, ledger or graph store implementations, transport crates. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- Every parser **reports its capability and version**.
- Parse facts **include evidence spans** where the parser can produce them.
- Dependency and schema parsers **emit graph candidates**.
- Every graph candidate carries **enough evidence to audit why the edge exists**.
- **Unsupported content degrades cleanly** — never blocks ingestion.
- Code parsing is **AST-backed** wherever a parser exists.

## DTO ownership
Wire DTOs (`SourceParseFacts`, `GraphCandidate`, `ParseEvidence`,
`CodeSymbolFact`, `DependencyFact`, `ApiSchemaFact`, `SessionFact`, …) are defined
in **`axon-api`**; this crate produces and returns them — it does not redefine
transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `sources/parsing-contract.md` ·
`sources/source-graph.md` · `schemas/graph-schema.md` (candidate examples) ·
`sources/metadata-payload.md` · the parse/candidate DTO components in `axon-api`.
