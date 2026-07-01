# PR8 Plan: Document Parse And Chunk Pipeline

Issue: [#298](https://github.com/jmagar/axon/issues/298)
Branch: `codex/document-parse-chunk`
Base: `main` after PR7 ledger lifecycle

## Goal

Implement the planned PR8 slice from issue #298:

> **Document/parse/chunk pipeline** — implement parser registry,
> `SourceParseFacts`, `GraphCandidate`, `DocumentPreparer`, `ChunkRouter`,
> code/tree-sitter, Markdown, transcript, session, package, API/schema, and
> plain-text routing.

This PR creates the shared document intelligence boundary. It must not cut over
the public CLI/MCP/REST source surfaces yet. Existing runtime behavior continues
through compatibility delegates while the new crates become the source of truth
for parse/chunk preparation.

## Non-Negotiables

- Use TDD: write failing tests for each behavior slice before implementation.
- Keep implementation files under 500 lines; split modules before they become
  dumping grounds.
- Keep tests in sibling `*_tests.rs` or `tests/*.rs` files, not inline module
  tests.
- Do not edit `CLAUDE.md`, `AGENTS.md`, or `GEMINI.md`.
- Do not wire public CLI/MCP/REST cutover in this PR.
- Do not persist graph rows in this PR; emit `GraphCandidate` values only.
- Do not move vector writes in this PR; existing embedding/runtime paths may
  delegate into the new document boundary.
- Before merge, re-read issue #298 and verify every active PR8 checklist item
  item-by-item against the PR head, then post the audit.

## Current-State Anchors

- Current runtime `SourceDocument -> PreparedDoc` lives in
  `crates/axon-vector/src/ops/source_doc.rs`.
- Current runtime tests live in
  `crates/axon-vector/src/ops/source_doc_tests.rs`.
- Current embedding input preparation lives in
  `crates/axon-vector/src/ops/tei/prepare.rs` and should remain there for PR8;
  it owns file reads, remote fetches, manifest reuse, config, and chunk-volume
  guards.
- Current TEI/Qdrant payload construction consumes legacy `PreparedDoc`; PR8
  must preserve positional alignment between chunks, `chunk_extra`, and
  explicit point ids while introducing the target DTO boundary.
- Target DTOs already exist in `axon-api::source`:
  - `SourceDocument`, `PreparedDocument`, `PreparedChunk`, `ChunkLocator` in
    `crates/axon-api/src/source/document.rs`.
  - `SourceParseFacts`, `GraphCandidate`, graph candidate evidence shapes in
    `crates/axon-api/src/source/graph.rs`.
  - `EmbeddingBatch` and vector DTOs in
    `crates/axon-api/src/source/vector.rs`.
- Target crates currently exist as skeletons:
  - `crates/axon-parse`
  - `crates/axon-document`
- Preserve current behavioral edge cases while moving the boundary:
  - crawl manifests whose URLs end in code-like extensions stay markdown/plain,
    not code.
  - git/local file preparation keeps document `content_type = "text"` even when
    chunk-level `chunk_content_kind` is markdown or plain text.
  - memory/atomic documents preserve explicit point ids.
  - `LedgerPayload` remains sealed and must not be spoofable through extra
    metadata.

## Implementation Order

### 1. Plan And Contract Gate

- [ ] Capture this PR8 plan.
- [ ] Dispatch read-only agents for:
  - current prepare/chunk implementation map
  - API DTO gap audit
  - source-specific parser/chunker inventory
  - schema/layering/checklist audit
- [ ] Fold agent findings into this plan before implementation if they expose a
  blocker.
- [ ] Resolve the API DTO gap before parser/document implementation:
  `PreparedDocument`, `PreparedChunk`, `SourceParseFacts`, `GraphCandidate`,
  `GraphEvidence`, `SourceRange`, `EmbeddingBatch`, and parse/embed stage
  results must match the stricter parsing/chunking/schema contracts.

Verification:

- [ ] `git diff --check`

### 2. `axon-parse` Core Types And Registry

Failing tests first:

- [ ] `ParserRegistry` selects parsers by explicit hint, content kind, MIME,
  file path/extension, and content sniffing.
- [ ] Unsupported content degrades to a no-op parser result with a warning, not
  a hard failure.
- [ ] Parser result serializes through `axon-api::source::SourceParseFacts` and
  `GraphCandidate`.
- [ ] Graph candidates use deterministic candidate/evidence keys.

Implementation:

- [ ] Update `axon-api::source` first if DTO fields are missing:
  - `SourceParseFacts` carries parser identity/method/provenance either directly
    or through typed metadata helpers.
  - parse stage results carry warnings/errors.
  - `GraphCandidate` exposes candidate kind/merge key semantics while retaining
    node/edge batches.
  - `GraphEvidence` can reference document, chunk, range, quote, and evidence
    kind.
  - `SourceRange` covers char ranges and transcript/session turn ranges.
- [ ] Add schema tests that assert PR8 DTO definitions are exported.
- [ ] Define `SourceParser`, `ParseInput`, `ParseResult`, `ParserCapability`,
  `ParserRegistry`, and parser warning/error helpers in `axon-parse`.
- [ ] Re-export API DTOs from `axon-api::source` rather than defining competing
  parse/graph wire shapes.
- [ ] Add `FakeParser` and `FakeParserRegistry` for downstream tests.
- [ ] Keep `axon-parse` free of vector, job, transport, ledger, and graph-store
  persistence dependencies.

Verification:

- [ ] `cargo test -p axon-parse --locked`
- [ ] `cargo xtask check-layering`

### 3. `axon-parse` Parser Families

Failing tests first:

- [ ] Code parser emits symbol facts with language, path, source range, parser
  method, confidence, and optional parent/visibility metadata.
- [ ] Code parser emits graph candidates for file/symbol containment.
- [ ] Manifest/package parsers emit dependency facts and dependency graph
  candidates for at least Cargo, npm/package.json, Python requirements/pyproject,
  Dockerfile, docker-compose, `.env.example`, OpenAPI/Swagger, GraphQL, protobuf,
  Terraform/Helm/Kubernetes YAML fixtures.
- [ ] Markdown parser emits heading/anchor facts.
- [ ] Transcript/session parser emits session/turn/tool-call/skill/agent facts
  and graph candidates.
- [ ] CLI/MCP tool output parser emits command/tool/request/response facts,
  artifact references for oversized output, and warnings for redacted fields.

Implementation:

- [ ] Wrap or reuse existing code intelligence where it exists; do not duplicate
  working tree-sitter/chunk parser logic unnecessarily.
- [ ] Use AST/tree-sitter/native parsers where already available.
- [ ] Allow regex fallback only with explicit `parser_method=regex_fallback`,
  confidence below `0.75`, and a warning.
- [ ] Keep source-specific parsing behind parser modules, not adapter code.

Verification:

- [ ] `cargo test -p axon-parse --locked`
- [ ] Golden fixtures for parser families

### 4. `axon-document` Core Types, Router, And Preparer

Failing tests first:

- [ ] `ChunkRouter` chooses profiles from explicit hint, content kind, MIME,
  path/extension, source kind/scope, structured payload, and document size.
- [ ] Every required profile routes explicitly:
  - `code_symbol`
  - `code_manifest`
  - `markdown_sections`
  - `html_article`
  - `plain_text_windows`
  - `transcript_segments`
  - `structured_records`
  - `api_schema`
  - `tool_output`
  - `session_turns`
  - `atomic_metadata`
- [ ] Unsupported/binary content produces bounded metadata-only or plain-text
  fallback with warnings.
- [ ] `DocumentPreparer` returns `PreparedDocument` with deterministic chunk
  ids, chunk hashes, locators, source ranges, cleanup keys, graph refs, and
  warnings/errors.

Implementation:

- [ ] Update `axon-api::source::{PreparedDocument, PreparedChunk}` first if DTO
  fields are missing:
  - prepared document canonical URI, prepare version, chunking profile,
    chunking method, parse facts, graph candidates, warnings, and errors
  - chunk key, content, content hash, optional embedding text, locator, source
    range, graph refs, parent/neighbor refs, title, and metadata
- [ ] Reject invalid prepared output before embedding: empty prepared documents,
  missing chunk keys, duplicate chunk ids/keys, impossible ranges, and unknown
  external fields.
- [ ] Decide and document dependency additions for `axon-document` before moving
  logic: likely `axon-api`, `axon-core`, `serde_json`, `tokio`, `uuid`,
  `text-splitter`, `pulldown-cmark`, URL helpers, and tree-sitter support. Do
  not pull in vector, job, transport, or store crates.
- [ ] Define `DocumentPreparer`, `PrepareSourceDocumentRequest`,
  `PrepareSourceDocumentResult`, `ChunkRouter`, `ChunkingProfile`,
  `PreparedChunkBuilder`, metadata helpers, and `FakeDocumentPreparer`.
- [ ] Use `axon-api::source::{SourceDocument, PreparedDocument,
  PreparedChunk}` as the external DTOs.
- [ ] Keep `axon-document` free of embedding provider, vector store, job runtime,
  transports, and graph-store persistence.

Verification:

- [ ] `cargo test -p axon-document --locked`
- [ ] `cargo xtask check-layering`

### 5. Move Runtime Preparation Behind New Boundary

Failing tests first:

- [ ] Current `axon-vector::ops::source_doc` tests remain green through a
  compatibility delegate.
- [ ] New `axon-document` tests prove the same chunk metadata for local/git code,
  markdown, plain text, memory/atomic text, and vertical structured payloads.
- [ ] Local legacy cleanup behavior remains intact until the later vector payload
  cutover owns cleanup keys fully.

Implementation:

- [ ] Move shared chunk offset/range/locator logic into `axon-document`.
- [ ] Move current file/code/markdown/plain/atomic routing into
  `axon-document`.
- [ ] Leave `axon-vector::ops::source_doc::prepare_source_document` as a thin
  delegate/adaptor to preserve current runtime call sites until public cutover.
- [ ] Convert between legacy `PreparedDoc` and target `PreparedDocument` only at
  the compatibility boundary.
- [ ] Update the existing source-doc audit test so direct preparation/chunking is
  allowed in `axon-document` and remains forbidden elsewhere.
- [ ] Avoid introducing a dependency cycle: `axon-vector` may depend on
  `axon-document`; `axon-document` must not depend on `axon-vector`.

Verification:

- [ ] `cargo test -p axon-document --locked`
- [ ] `cargo test -p axon-vector source_doc --locked`
- [ ] `cargo xtask check-layering`

### 6. Chunk Profile Coverage And Fixtures

Failing tests first:

- [ ] Code/tree-sitter chunks keep symbol name/kind, language, line range,
  byte range, parser/chunker method, and test/generated flags.
- [ ] Manifest chunks are structured, small, and dependency-oriented.
- [ ] Markdown chunks preserve heading paths, anchors, fenced code metadata, and
  table handling where supported.
- [ ] Transcript/session chunks preserve timestamps, speaker/role, turn ids,
  tool-call ids, skills invoked, agents invoked, and decision facts.
- [ ] API/schema chunks preserve endpoint/type/method/rpc/field facts.
- [ ] Plain text chunks are bounded and deterministic.
- [ ] Repomix-style packed repository content is split by original file section
  before chunking; the packed output is not indexed as one giant document.

Implementation:

- [ ] Add fixtures under crate-owned test fixture directories.
- [ ] Keep large fixture bodies compact enough for quick local checks.
- [ ] Add warnings where profiles degrade rather than failing the whole document.

Verification:

- [ ] `cargo test -p axon-document --locked`
- [ ] `cargo test -p axon-parse --locked`

### 7. Schema And Contract Sync

- [ ] Add any new public types/enums to generated schema inputs.
- [ ] Refresh generated API/schema docs if DTOs changed.
- [ ] Add schema drift tests for parse/chunk DTOs and profile names.
- [ ] Ensure no removed/public surface changes sneak into this PR.

Verification:

- [ ] `cargo xtask schemas generate --check`
- [ ] `cargo test -p xtask schemas --locked`
- [ ] `cargo xtask check-repo-structure`
- [ ] `cargo xtask check-doc-contracts`
- [ ] `cargo xtask check-doc-links`

### 8. Review, Hardening, And PR Gate

- [ ] Run a changed-file LOC check; split any Rust file over 500 lines.
- [ ] Run local verification commands.
- [ ] Push and open the PR.
- [ ] Run mandatory `lavra-review` on the PR and address all findings.
- [ ] Dispatch PR review toolkit agents and address all findings.
- [ ] Confirm required GitHub checks are green.
- [ ] Re-read issue #298 and audit the active PR8 checklist item-by-item:
  - parser registry
  - `SourceParseFacts`
  - `GraphCandidate`
  - `DocumentPreparer`
  - `ChunkRouter`
  - code/tree-sitter routing
  - Markdown routing
  - transcript routing
  - session routing
  - package/manifest routing
  - API/schema routing
  - plain-text routing
- [ ] Post the final pre-merge gate audit to the PR/issue.
- [ ] Merge only after the audit and required checks are green.

## Expected Verification Set

Run the narrow checks as development proceeds:

```bash
cargo test -p axon-parse --locked
cargo test -p axon-document --locked
cargo test -p axon-vector source_doc --locked
cargo test -p axon-api source --locked
cargo test -p xtask schemas --locked
cargo xtask schemas generate --check
cargo xtask check-layering
cargo xtask check-repo-structure
cargo xtask check-doc-contracts
cargo xtask check-doc-links
cargo fmt --all --check
git diff --check
```

Use broader checks only when code movement affects additional crates.
