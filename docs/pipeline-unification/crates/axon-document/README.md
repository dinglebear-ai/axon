# axon-document Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-document` owns document preparation: routing source documents to the right
chunking strategy and producing `PreparedDocument` values for embedding.

## Owns

- `DocumentPreparer` and `ChunkRouter`
- source-kind/content-kind chunking profiles
- code-aware, markdown-aware, transcript-aware, session-aware, schema-aware,
  and generic text chunking
- prepared document and chunk construction helpers
- chunk metadata normalization and payload preflight validation

## Must Not Own

- source acquisition, parsing facts persistence, embedding provider calls, vector
  store writes, or final retrieval ranking
- transport rendering or job scheduling
- AST parser implementation details beyond consuming `SourceParseFacts`

## Public Modules

```text
lib.rs
preparer.rs
chunk_router.rs
profile.rs
prepared.rs
chunk.rs
metadata.rs
code.rs
markdown.rs
transcript.rs
session.rs
schema.rs
text.rs
testing.rs
```

## Public API

- `DocumentPreparer`
- `ChunkRouter`
- `ChunkingProfile`
- `PreparedDocument`
- `PreparedChunk`
- `ChunkMetadata`
- `PrepareSourceDocumentRequest`
- `PrepareSourceDocumentResult`
- `FakeDocumentPreparer`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-parse`, `axon-observe`
- tokenizer/text splitting crates

## Dependencies Forbidden

- embedding providers, vector stores, LLM providers, job runtime, transports
- acquisition adapters as concrete dependencies

## Generated Artifacts

- chunking profile registry in [../../sources/chunking-contract.md](../../sources/chunking-contract.md)
- prepared-document DTO schema fixtures

## Fixtures And Fakes

- code chunk fixture with symbol boundaries
- markdown heading chunk fixture
- session turn/tool-call chunk fixture
- OpenAPI/schema chunk fixture
- fake preparer returning deterministic chunk ids

## Tests

- chunk ids are stable for unchanged source items
- chunk metadata includes source id, item key, generation, content kind, parser
  version, chunking method, line/span information when available
- every supported content kind routes to an explicit profile
- unsupported content degrades to bounded text chunking

## Acceptance Criteria

- all adapters emit `SourceDocument`; only this crate emits `PreparedDocument`
- no embedding or vector store code appears here
- source-specific optimization is hidden behind one preparation boundary

See [../README.md](../README.md) and
[../../sources/chunking-contract.md](../../sources/chunking-contract.md).
