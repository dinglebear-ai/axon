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

- [x] Capture this PR8 plan.
- [x] Dispatch read-only agents for:
  - current prepare/chunk implementation map
  - API DTO gap audit
  - source-specific parser/chunker inventory
  - schema/layering/checklist audit
- [x] Fold agent findings into this plan before implementation if they expose a
  blocker.
- [x] Resolve the API DTO gap before parser/document implementation:
  `PreparedDocument`, `PreparedChunk`, `SourceParseFacts`, `GraphCandidate`,
  `GraphEvidence`, `SourceRange`, `EmbeddingBatch`, and parse/embed stage
  results must match the stricter parsing/chunking/schema contracts.

Verification:

- [x] `git diff --check`

### 2. `axon-parse` Core Types And Registry

Failing tests first:

- [x] `ParserRegistry` selects parsers by explicit hint, content kind, MIME,
  file path/extension, and content sniffing.
- [x] Unsupported content degrades to a no-op parser result with a warning, not
  a hard failure.
- [x] Parser result serializes through `axon-api::source::SourceParseFacts` and
  `GraphCandidate`.
- [x] Graph candidates use deterministic candidate/evidence keys.

Implementation:

- [x] Update `axon-api::source` first if DTO fields are missing:
  - `SourceParseFacts` carries parser identity/method/provenance either directly
    or through typed metadata helpers.
  - parse stage results carry warnings/errors.
  - `GraphCandidate` exposes candidate kind/merge key semantics while retaining
    node/edge batches.
  - `GraphEvidence` can reference document, chunk, range, quote, and evidence
    kind.
  - `SourceRange` covers char ranges and transcript/session turn ranges.
- [x] Add schema tests that assert PR8 DTO definitions are exported.
- [x] Define `SourceParser`, `ParseInput`, `ParseResult`, `ParserCapability`,
  `ParserRegistry`, and parser warning/error helpers in `axon-parse`.
- [x] Re-export API DTOs from `axon-api::source` rather than defining competing
  parse/graph wire shapes.
- [x] Add `FakeParser` and `FakeParserRegistry` for downstream tests.
- [x] Keep `axon-parse` free of vector, job, transport, ledger, and graph-store
  persistence dependencies.

Verification:

- [x] `cargo test -p axon-parse --locked`
- [x] `cargo xtask check-layering`

### 3. `axon-parse` Parser Families

Failing tests first:

- [x] Code parser emits symbol facts with language, path, source range, parser
  method, confidence, and optional parent/visibility metadata.
- [x] Code parser emits graph candidates for file/symbol containment.
- [x] Manifest/package parsers emit dependency facts and dependency graph
  candidates for at least Cargo, npm/package.json, Python requirements/pyproject,
  Dockerfile, docker-compose, `.env.example`, OpenAPI/Swagger, GraphQL, protobuf,
  Terraform/Helm/Kubernetes YAML fixtures.
- [x] Markdown parser emits heading/anchor facts.
- [x] Transcript/session parser emits session/turn/tool-call/skill/agent facts
  and graph candidates.
- [x] CLI/MCP tool output parser emits command/tool/request/response facts,
  artifact references for oversized output, and warnings for redacted fields.

Implementation:

- [x] Wrap or reuse existing code intelligence where it exists; do not duplicate
  working tree-sitter/chunk parser logic unnecessarily.
- [x] Use AST/tree-sitter/native parsers where already available. Current
  runtime code preparation keeps the existing tree-sitter chunking path through
  the vector compatibility adapter; new parse facts that use heuristics are
  marked below high-confidence parser output.
- [x] Allow regex/line heuristic fallback only with explicit fallback/heuristic
  parser methods and confidence below `0.75`.
- [x] Keep source-specific parsing behind parser modules, not adapter code.

Verification:

- [x] `cargo test -p axon-parse --locked`
- [x] Compact golden-style fixtures for parser families

### 4. `axon-document` Core Types, Router, And Preparer

Failing tests first:

- [x] `ChunkRouter` chooses profiles from explicit hint, content kind, MIME,
  path/extension, source kind/scope, structured payload, and document size.
- [x] Every required profile routes explicitly:
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
- [x] Unsupported/binary content produces bounded metadata-only or plain-text
  fallback with warnings.
- [x] `DocumentPreparer` returns `PreparedDocument` with deterministic chunk
  ids, chunk hashes, locators, source ranges, cleanup keys, graph refs, and
  warnings/errors.

Implementation:

- [x] Update `axon-api::source::{PreparedDocument, PreparedChunk}` first if DTO
  fields are missing:
  - prepared document canonical URI, prepare version, chunking profile,
    chunking method, parse facts, graph candidates, warnings, and errors
  - chunk key, content, content hash, optional embedding text, locator, source
    range, graph refs, parent/neighbor refs, title, and metadata
- [x] Reject invalid prepared output before embedding: empty prepared documents,
  missing chunk keys, duplicate chunk ids/keys, impossible ranges, and unknown
  external fields.
- [x] Decide and document dependency additions for `axon-document` before moving
  logic. PR8 keeps the dependency set intentionally small: `axon-api` and
  `serde_json`; it does not pull in vector, job, transport, or store crates.
- [x] Define `DocumentPreparer`, `PrepareSourceDocumentRequest`,
  `PrepareSourceDocumentResult`, `ChunkRouter`, `ChunkingProfile`,
  metadata helpers, and `FakeDocumentPreparer`.
- [x] Use `axon-api::source::{SourceDocument, PreparedDocument,
  PreparedChunk}` as the external DTOs.
- [x] Keep `axon-document` free of embedding provider, vector store, job runtime,
  transports, and graph-store persistence.

Verification:

- [x] `cargo test -p axon-document --locked`
- [x] `cargo xtask check-layering`

### 5. Move Runtime Preparation Behind New Boundary

Failing tests first:

- [x] Current `axon-vector::ops::source_doc` tests remain green through a
  compatibility delegate.
- [x] New `axon-document` tests prove target chunk metadata for code,
  markdown, plain text, memory/atomic text, and vertical structured payloads.
- [x] Local legacy cleanup behavior remains intact until the later vector payload
  cutover owns cleanup keys fully.

Implementation:

- [x] Move shared chunk offset/range/locator logic into `axon-document` for the
  target `PreparedDocument` path.
- [x] Move atomic/memory compatibility routing into `axon-document`. File,
  code, markdown, and plain-text legacy payload preparation intentionally remain
  in `axon-vector` behind a TODO(PR8/#298) audit allowance until PR9/vector
  payload and PR11/source-family cutover can move them without changing Qdrant
  payload shape.
- [x] Leave `axon-vector::ops::source_doc::prepare_source_document` as a thin
  delegate/adaptor to preserve current runtime call sites until public cutover.
- [x] Convert between legacy `PreparedDoc` and target `PreparedDocument` only at
  the compatibility boundary.
- [x] Update the existing source-doc audit test so direct preparation/chunking is
  allowed in `axon-document` and remains forbidden elsewhere.
- [x] Avoid introducing a dependency cycle: `axon-vector` may depend on
  `axon-document`; `axon-document` must not depend on `axon-vector`.

Verification:

- [x] `cargo test -p axon-document --locked`
- [x] `cargo test -p axon-vector source_doc --locked`
- [x] `cargo xtask check-layering`

### 6. Chunk Profile Coverage And Fixtures

Failing tests first:

- [x] Code/tree-sitter chunks keep symbol name/kind, language, line range,
  byte range, parser/chunker method, and test/generated flags.
- [x] Manifest chunks are structured, small, and dependency-oriented.
- [x] Markdown chunks/facts preserve heading paths and anchors. Fenced code and
  table-specific semantics are left for richer source-family parsers where
  needed.
- [x] Transcript/session chunks/facts preserve speaker/role, turn ids,
  tool-call ids, skills invoked, and agents invoked. Timestamp-specific session
  parsing is left for richer session-source fixtures where needed.
- [x] API/schema chunks preserve endpoint/type/method/rpc/field facts.
- [x] Plain text chunks are bounded and deterministic.
- [x] Repomix-style packed repository content is split by original file section
  before chunking; the packed output is not indexed as one giant document.

Implementation:

- [x] Add compact fixtures in crate-owned sibling tests.
- [x] Keep large fixture bodies compact enough for quick local checks.
- [x] Add warnings where profiles degrade rather than failing the whole document.

Verification:

- [x] `cargo test -p axon-document --locked`
- [x] `cargo test -p axon-parse --locked`

### 7. Schema And Contract Sync

- [x] Add any new public types/enums to generated schema inputs.
- [x] Refresh generated API/schema docs if DTOs changed.
- [x] Add schema drift tests for parse/chunk DTOs and profile names.
- [x] Ensure no removed/public surface changes sneak into this PR.

Verification:

- [x] `cargo xtask schemas generate --check`
- [x] `cargo test -p xtask schemas --locked`
- [x] `cargo xtask check-repo-structure`
- [x] `cargo xtask check-doc-contracts`
- [x] `cargo xtask check-doc-links`

### 8. Review, Hardening, And PR Gate

- [x] Run a changed-file LOC check; split any Rust file over 500 lines.
- [x] Run local verification commands.
- [x] Push and open the PR.
- [x] Run mandatory `lavra-review` on the PR and address all findings.
- [x] Dispatch PR review toolkit agents and address all findings.
- [x] Confirm required GitHub checks are green.
- [x] Re-read issue #298 and audit the active PR8 checklist item-by-item:
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
- [x] Post the final pre-merge gate audit to the PR/issue.
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
