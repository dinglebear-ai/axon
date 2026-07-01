# axon-document — Agent Guide

`axon-document` owns **document preparation**: routing source documents to the
right chunking strategy and producing `PreparedDocument`/`PreparedChunk` values
for embedding. It consumes `SourceParseFacts`; it does not embed, persist, or
acquire. Full contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-document/README.md](../../../docs/pipeline-unification/crates/axon-document/README.md)
· chunking profile registry:
[../../../docs/pipeline-unification/sources/chunking-contract.md](../../../docs/pipeline-unification/sources/chunking-contract.md).

## Status — PR0 skeleton
Modules below are **markers only**. Real implementation lands in **Phase 7**,
decomposed out of `axon-vector`'s existing chunk-preparation logic. Do not add
embedding-provider calls, vector writes, or acquisition behavior here.

## Module map
| File | Owns |
|---|---|
| `preparer.rs` | `DocumentPreparer` — the one boundary that emits `PreparedDocument` |
| `chunk_router.rs` | `ChunkRouter` — routes source-kind/content-kind to a profile |
| `profile.rs` | `ChunkingProfile` — per-content-kind chunking strategy |
| `prepared.rs` | `PreparedDocument`, `PreparedChunk` construction helpers |
| `chunk.rs` / `metadata.rs` | chunk builders + `ChunkMetadata` normalization + payload preflight |
| `code.rs` | code-aware (AST/symbol-boundary) chunking |
| `markdown.rs` | markdown/heading-aware chunking |
| `transcript.rs` / `session.rs` | transcript- and session-turn/tool-call chunking |
| `schema.rs` | OpenAPI/schema-aware chunking |
| `text.rs` | generic bounded text fallback |
| `testing.rs` | `FakeDocumentPreparer` — deterministic chunk ids/fixtures |

## Boundary — keep OUT of this crate
- Source acquisition, parse-facts persistence, embedding provider calls, vector store writes, retrieval ranking.
- Transport rendering and job scheduling.
- AST parser implementation details beyond consuming `SourceParseFacts`.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-parse`, `axon-observe`, tokenizer/text-splitting crates.
- **Forbidden:** embedding providers (`axon-embedding`), vector stores, LLM providers, job runtime, transport crates, acquisition adapters as concrete deps. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- All adapters still emit `SourceDocument`; **only this crate emits `PreparedDocument`**.
- **Chunk ids are stable** for unchanged source items.
- Chunk metadata carries source id, item key, generation, content kind, parser version, chunking method, and line/span info when available.
- Every supported content kind routes to an **explicit profile**; unsupported content degrades to **bounded text chunking**.
- Source-specific optimization stays hidden behind this single preparation boundary.

## DTO ownership
Serializable wire shapes (`PreparedDocument`/`PreparedChunk`/`ChunkMetadata`
components) are defined in **`axon-api`**; this crate builds and returns them via
constructors/builders — it does not redefine transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `sources/chunking-contract.md` (profile registry) ·
`sources/metadata-payload.md` · the prepared-document DTO components in `axon-api`.
