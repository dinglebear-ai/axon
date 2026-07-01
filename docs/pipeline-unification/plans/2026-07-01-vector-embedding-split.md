# PR9 Plan: Vector And Embedding Split

> **Status:** Active PR #315. Completed task checkboxes mark implemented work;
> final merge-gate items remain unchecked until the pre-merge audit, required
> checks, and merge actually complete.
>
> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

Issue: [#298](https://github.com/jmagar/axon/issues/298)
Branch: `codex/vector-embedding-split`
Base: `main` after PR8 document/parse/chunk pipeline

## Goal

Implement the planned PR9 slice from issue #298:

> **Vector/embedding split** — introduce `EmbeddingProvider`,
> `VectorStore`, `VectorPointBatch`, shared payload builder, Qdrant
> implementation, vector index definitions, and payload/redaction fixtures.

This PR establishes the target embedding and vector boundaries while preserving
current runtime behavior through compatibility adapters. It must not cut over
CLI/MCP/REST surfaces, delete `axon-vector`, or rewrite RAG synthesis.

## Architecture

Current runtime vector behavior is still concentrated in `axon-vector`. PR9
extracts stable target boundaries into `axon-embedding`, `axon-vectors`, and the
retrieval-facing DTO/test foundations in `axon-retrieval`, then bridges current
source-document payload construction through those target shapes where safe.
The legacy `axon-vector` Qdrant source publish/count path remains unchanged in
this PR because the current runtime still writes numeric `source_generation` and
`source_index_version` payload fields. The later source-family cutover must
replace that legacy publisher with the target generation publisher before opaque
string generations become the live write path.

`axon-embedding` owns embedding provider contracts, deterministic fakes, batch
formation, provider capability, typed reservation-facing metadata, and
non-wired TEI/OpenAI-compatible adapter shells. `axon-vectors` owns collection
specs, vector point batches, payload validation, filter/delete/search traits,
fake store behavior, and a test-only Qdrant conversion boundary shell.
`axon-retrieval` stays a boundary crate only in PR9: it may add retrieval
DTO/fake contracts needed by vector-store search tests, but final
query/retrieve/ask movement is a later PR.

## Tech Stack

Rust 2024, `async_trait`, `serde`, `schemars`, `utoipa`, `qdrant-client`,
`axon-api::source` DTOs, `axon-document` prepared documents, `axon-embedding`
provider fakes, `axon-vectors` vector store fakes and payload builder, existing
`axon-vector` runtime compatibility path.

## Global Constraints

- Use TDD: every production behavior change starts with a failing sibling test.
- Do not edit `CLAUDE.md`, `AGENTS.md`, or `GEMINI.md`.
- Keep production Rust modules under 500 LOC; split modules before they become dumping grounds. Schema-generator tooling under `xtask/src/schemas/**` may exceed this briefly when one generator owns one emitted contract family.
- Use sibling `*_tests.rs` files; do not add inline `#[cfg(test)] mod tests`.
- Do not wire public CLI/MCP/REST cutover in this PR.
- Do not remove `axon-vector`, `axon-code-index`, `axon-crawl`, `axon-ingest`, or `axon-extract` in this PR.
- Do not move final RAG synthesis or LLM calls into `axon-retrieval` in this PR.
- Do not make old vectors searchable through a new public query path.
- Do not introduce compatibility aliases for removed future surfaces.
- No Qdrant writes may bypass a validated `VectorPointBatch` in new target code.
- Payloads must reject secrets, unknown source-specific fields, missing generation fields, and bad visibility before vector write.
- Embedding generation must not know Qdrant payload shape or vector-store write details.
- Vector-store code must not call TEI/OpenAI embedding providers.
- Retrieval code must depend on store/provider traits, not concrete Qdrant/TEI internals.
- Current `axon-vector` runtime remains the live path until the later source-family and surface cutover PRs.
- Commit early after each task's verification passes.

## Current-State Anchors

- Current embedding and Qdrant runtime lives under `crates/axon-vector/src/ops/`.
- Current source-document bridge lives under `crates/axon-vector/src/ops/source_doc/`.
- Target embedding skeleton: `crates/axon-embedding/src/{provider,batch,capability,reservation,fake,tei,openai_compat,testing}.rs`.
- Target vector skeleton: `crates/axon-vectors/src/{store,collection,point,payload,filter,query,health,qdrant,testing}.rs`.
- Target retrieval skeleton: `crates/axon-retrieval/src/{engine,plan,query,filter,rank,context,citation,graph,memory,testing}.rs`.
- Target DTOs already exist in `crates/axon-api/src/source/vector.rs` and `crates/axon-api/src/source/document.rs`.
- Payload schema contract: `docs/pipeline-unification/schemas/vector-payload-schema.md`.
- Metadata registry contract: `docs/pipeline-unification/sources/metadata-payload.md`.

## Non-Goals

- No public `query`, `retrieve`, `ask`, `embed`, or source command behavior changes.
- No live Qdrant migration.
- No Qdrant collection recreation.
- No removal of current `axon-vector` internals.
- No full `axon-retrieval` RAG port.
- No ask-context, citation rendering, source display, or LLM synthesis movement.
- No old-data migration/backfill.

## Task 1: Tighten `axon-embedding` Provider Boundary

**Files:**

- Modify: `crates/axon-embedding/src/batch.rs`
- Modify: `crates/axon-embedding/src/capability.rs`
- Modify: `crates/axon-embedding/src/provider.rs`
- Modify: `crates/axon-embedding/src/fake.rs`
- Modify: `crates/axon-embedding/src/tei.rs`
- Modify: `crates/axon-embedding/src/openai_compat.rs`
- Modify: `crates/axon-embedding/src/testing.rs`
- Test: `crates/axon-embedding/src/provider_tests.rs`

**Interfaces:**

- Consumes: `axon_api::source::{EmbeddingBatch, EmbeddingInput, EmbeddingResult, EmbeddingVector, ProviderCapability}`
- Produces: reusable `EmbeddingBatchBuilder`, `EmbeddingBatchValidation`, deterministic fake vectors, provider capability helpers.

- [x] Write failing tests for batch validation:
  - empty batch is rejected
  - duplicate chunk ids are rejected
  - blank embedding text is rejected
  - mixed content kinds are accepted but order is preserved

- [x] Run `cargo test -p axon-embedding batch --locked` and confirm the new tests fail for missing validation.

- [x] Implement `EmbeddingBatchBuilder` and validation helpers in `batch.rs`.

- [x] Write failing tests for fake provider behavior:
  - fake output is deterministic for the same chunk id and dimensions
  - fake output preserves input order
  - fake rejects zero dimensions
  - fake records calls without exposing mutable internals

- [x] Run `cargo test -p axon-embedding fake --locked` and confirm the new tests fail for missing behavior.

- [x] Implement minimal fake/provider fixes.

- [x] Add provider capability helper constructors in `capability.rs` so callers do not hand-roll capability metadata.

- [x] Write failing adapter-shell tests:
  - TEI adapter config preserves endpoint, model, dimensions, timeout, max batch inputs, and instruction support
  - OpenAI-compatible adapter config preserves base URL, model, dimensions, timeout, and batch limits
  - adapter shells expose capabilities without making network calls
  - live `embed` calls return `provider.not_wired` until current runtime TEI plumbing is deliberately moved

- [x] Run `cargo test -p axon-embedding provider --locked` and confirm the new adapter-shell tests fail.

- [x] Implement TEI/OpenAI-compatible adapter shells with capability reporting only; do not move live current runtime embedding calls yet.

- [x] Run `cargo test -p axon-embedding --locked`.

- [x] Commit: `feat(embedding): harden provider batch boundary`.

## Task 2: Add `axon-vectors` Payload Registry And Validation

**Files:**

- Modify: `crates/axon-vectors/src/payload.rs`
- Modify: `crates/axon-vectors/src/point.rs`
- Modify: `crates/axon-vectors/src/lib.rs`
- Test: `crates/axon-vectors/src/payload_tests.rs`
- Create fixtures:
  - `crates/axon-vectors/tests/fixtures/payload/code.valid.json`
  - `crates/axon-vectors/tests/fixtures/payload/web.valid.json`
  - `crates/axon-vectors/tests/fixtures/payload/session.valid.json`
  - `crates/axon-vectors/tests/fixtures/payload/memory.valid.json`
  - `crates/axon-vectors/tests/fixtures/payload/package.valid.json`
  - `crates/axon-vectors/tests/fixtures/payload/secret.invalid.json`
  - `crates/axon-vectors/tests/fixtures/payload/missing_source_generation.invalid.json`
  - `crates/axon-vectors/tests/fixtures/payload/unknown_source_field.invalid.json`
  - `crates/axon-vectors/tests/fixtures/payload/bad_visibility.invalid.json`

**Interfaces:**

- Consumes: `PreparedDocument`, `PreparedChunk`, `EmbeddingResult`, `VectorPointBatch`
- Produces: `VectorPayload`, `VectorPayloadValidationError`, source-specific field registry.

- [x] Write failing payload validation tests for the required vector-payload contract:
  - every payload has `payload_contract_version`, `collection`, `source_id`, `source_generation`, `document_id`, `chunk_id`, `chunk_locator`, `source_range`, `visibility`, `redaction_status`, `job_id`, `document_status`, `embedding_batch_id`, `embedding_model`, `embedding_dimensions`, `embedding_provider`, `embedding_profile`, and `embedded_at`
  - forbidden fields such as raw auth headers, cookies, API keys, raw `.env` values, absolute home paths, raw HTML blobs, and adapter response blobs are rejected
  - unknown source-specific fields are rejected unless they use an approved registry entry
  - `source_generation` and `committed_generation` are opaque keyword strings and filterable
  - bad visibility values are rejected

- [x] Run `cargo test -p axon-vectors payload --locked` and confirm the new tests fail.

- [x] Implement `VectorPayload` as a typed wrapper over `MetadataMap` with explicit required-field validation.

- [x] Implement source-specific field registry entries for initial PR9 families:
  - code: `code_language`, `code_symbol_name`, `code_symbol_kind`, `code_file_type`
  - web: `web_title`, `web_domain`, `web_status_code`, `web_depth`
  - package: `package_ecosystem`, `package_name`, `package_version`
  - session: `session_id`, `session_turn_index`, `session_tool_name`, `session_skill_name`
  - graph: `graph_node_ids`, `graph_edge_ids`, `graph_confidence`
  - memory: `memory_id`, `memory_importance`, `memory_status`

- [x] Implement fixture loader tests for every required valid/invalid fixture.

- [x] Run `cargo test -p axon-vectors payload --locked`.

- [x] Commit: `feat(vectors): add payload registry and validation`.

## Task 3: Build `VectorPointBatch` From Prepared Documents And Embeddings

**Files:**

- Modify: `crates/axon-vectors/src/point.rs`
- Modify: `crates/axon-vectors/src/payload.rs`
- Modify: `crates/axon-vectors/src/testing.rs`
- Test: `crates/axon-vectors/src/point_tests.rs`

**Interfaces:**

- Consumes: `PreparedDocument`, `EmbeddingResult`, `CollectionSpec`, payload metadata options
- Produces: deterministic `VectorPointBatch` with validated payloads.

- [x] Write failing tests:
  - one prepared document with two chunks and two embeddings produces two points
  - embedding chunk-id mismatch fails before producing a partial batch
  - duplicate chunk ids fail
  - point ids are stable for `(collection, vector_namespace, document_id, chunk_id, source_generation)`
  - embedding model changes update payload provenance without churning point ids
  - dimensions mismatch fails
  - payload validation runs before returning the batch

- [x] Run `cargo test -p axon-vectors point --locked` and confirm tests fail.

- [x] Implement `VectorPointBatchBuilder`.

- [x] Add test helpers in `testing.rs` for compact prepared-document and embedding-result fixtures.

- [x] Run `cargo test -p axon-vectors point --locked`.

- [x] Commit: `feat(vectors): build validated point batches`.

## Task 4: Harden `VectorStore` Fake, Filters, Collection Specs, And Delete Safety

**Files:**

- Modify: `crates/axon-vectors/src/store.rs`
- Modify: `crates/axon-vectors/src/collection.rs`
- Modify: `crates/axon-vectors/src/filter.rs`
- Modify: `crates/axon-vectors/src/query.rs`
- Modify: `crates/axon-vectors/src/health.rs`
- Test: `crates/axon-vectors/src/store_tests.rs`

**Interfaces:**

- Consumes: `CollectionSpec`, `VectorPointBatch`, `VectorDeleteSelector`, `VectorSearchRequest`
- Produces: deterministic fake store behavior and safe filter/delete helpers.

- [x] Write failing tests:
  - collection creation is idempotent for the same dimensions/vector names
  - collection creation rejects dimension/vector-name drift
  - payload indexes are recorded from `CollectionSpec`
  - fake search filters by source id, generation, document id, chunk id, namespace, visibility, and content kind
  - delete by source/generation/document/chunk/point selectors deletes only matching points
  - cleanup debt selectors cannot delete unrelated source/generation points
  - fake store can simulate unavailable, timeout, rate-limit, fatal, partial failure, and slow write modes

- [x] Run `cargo test -p axon-vectors store --locked` and confirm tests fail.

- [x] Implement collection-spec normalization and drift checks.

- [x] Implement typed filter helpers for indexed fields from the vector payload schema.

- [x] Implement selector support in fake store for source, generation, document, chunks, and points.

- [x] Run `cargo test -p axon-vectors --locked`.

- [x] Commit: `feat(vectors): harden vector store fake and filters`.

## Task 5: Add Qdrant Boundary Shell Without Public Runtime Cutover

**Files:**

- Modify: `crates/axon-vectors/Cargo.toml`
- Modify: `crates/axon-vectors/src/qdrant.rs`
- Modify: `crates/axon-vectors/src/collection.rs`
- Modify: `crates/axon-vectors/src/store.rs`
- Test: `crates/axon-vectors/src/qdrant_tests.rs`

**Interfaces:**

- Consumes: target `VectorStore` trait and `CollectionSpec`
- Produces: test-visible `QdrantVectorStore` constructor/config, collection/index planning helpers, request conversion helpers.

- [x] Write failing conversion tests that do not require a live Qdrant:
  - `CollectionSpec` converts to named dense vector config and optional sparse config
  - payload index specs convert to Qdrant index requests
  - source/generation/document filters convert to Qdrant filters using indexed payload fields
  - `VectorPointBatch` converts to Qdrant upsert points without dropping payload fields

- [x] Run `cargo test -p axon-vectors qdrant --locked` and confirm tests fail.

- [x] Implement `QdrantVectorStore` as a non-wired target implementation shell with conversion helpers and clear `vector.not_wired` live-call errors where live client plumbing is not yet moved.

- [x] Ensure no current runtime path imports `axon-vectors::qdrant::QdrantVectorStore` yet.

- [x] Run `cargo test -p axon-vectors qdrant --locked`.

- [x] Commit: `feat(vectors): add qdrant vector store boundary`.

## Task 6: Bridge Current Source-Document Payload Metadata To Target Batch Shape

**Files:**

- Modify: `crates/axon-vector/src/ops/source_doc/document_bridge.rs`
- Modify: `crates/axon-vector/src/ops/source_doc.rs`
- Modify: `crates/axon-vector/Cargo.toml`
- Test: `crates/axon-vector/src/ops/source_doc_tests.rs`
- Test: `crates/axon-vector/src/ops/source_doc_audit_tests.rs`

**Interfaces:**

- Consumes: existing legacy `PreparedDoc` and target `PreparedDocument`
- Produces: compatibility metadata that can be validated by `axon-vectors` without changing current Qdrant writes.

- [x] Write failing tests:
  - bridge metadata includes target prepared chunk id/key/hash, source id, source item key, source generation, document id, chunk id, and content hash
  - memory/atomic explicit point ids remain preserved
  - bridge does not copy raw absolute local paths into public payload identity
  - bridge output can be converted into a target `VectorPayload` fixture without adding unregistered fields

- [x] Run `cargo test -p axon-vector source_doc --locked` and confirm tests fail.

- [x] Implement minimal bridge metadata additions and target validation helper calls.

- [x] Keep `axon-vector` as current runtime owner; do not route live writes through `axon-vectors` yet.

- [x] Run `cargo test -p axon-vector source_doc --locked`.

- [x] Commit: `feat(vector): validate source-doc payload bridge`.

## Task 7: Add Retrieval Boundary Fakes For Later Query/Ask Cutover

**Files:**

- Modify: `crates/axon-retrieval/src/engine.rs`
- Modify: `crates/axon-retrieval/src/plan.rs`
- Modify: `crates/axon-retrieval/src/query.rs`
- Modify: `crates/axon-retrieval/src/context.rs`
- Modify: `crates/axon-retrieval/src/citation.rs`
- Modify: `crates/axon-retrieval/src/testing.rs`
- Test: `crates/axon-retrieval/src/engine_tests.rs`

**Interfaces:**

- Consumes: target `VectorStore` search results and `EmbeddingProvider` query embeddings
- Produces: `RetrievalEngine`, `RetrievalPlan`, deterministic fake retrieval result, citation/context DTO helpers.

- [x] Write failing tests:
  - retrieval plan preserves source id, generation, visibility, and namespace filters
  - ranking is deterministic with fixed fake vector search results
  - context assembly respects byte/token budget inputs
  - citations always include source id, document id, chunk id, canonical URI, and source range

- [x] Run `cargo test -p axon-retrieval --locked` and confirm tests fail.

- [x] Implement minimal boundary/fake retrieval engine without moving current query/ask runtime.

- [x] Run `cargo test -p axon-retrieval --locked`.

- [x] Commit: `feat(retrieval): add retrieval boundary fake`.

## Task 8: Schema, Docs, And Drift Checks

**Files:**

- Modify: `xtask/src/schemas/*` as needed
- Modify: `docs/reference/api/schemas.json`
- Modify: `docs/reference/api/dto.md`
- Create or modify: `docs/reference/sources/vector-payload.schema.json`
- Create or modify: `docs/reference/sources/vector-payload.md`
- Test: `xtask/src/schemas/tests.rs`

**Interfaces:**

- Consumes: `axon-api`, `axon-vectors` payload registry, vector-payload contract
- Produces: generated vector payload schema/check artifacts and source-input manifests.

- [x] Write failing schema tests:
  - vector payload generated schema includes every registered required field
  - generated Qdrant index plan references only schema fields
  - payload fixture examples validate through the same registry used by the builder
  - schema source input manifest includes payload builder, metadata contract, chunking contract, and API vector DTOs

- [x] Run `cargo test -p xtask schemas --locked` and confirm tests fail.

- [x] Implement the minimal generator/check updates.

- [x] Run `cargo xtask schemas generate`.

- [x] Run `cargo xtask schemas generate --check`.

- [x] Commit: `feat(schemas): generate vector payload contract`.

## Task 9: Verification, Reviews, And PR Gate

- [x] Run targeted tests:

```bash
cargo test -p axon-embedding --locked
cargo test -p axon-vectors --locked
cargo test -p axon-retrieval --locked
cargo test -p axon-vector source_doc --locked
cargo test -p axon-api source --locked
cargo test -p xtask schemas --locked
```

- [x] Run structural checks:

```bash
cargo xtask schemas generate --check
cargo xtask check-layering
cargo xtask check-repo-structure
cargo xtask check-doc-contracts
cargo xtask check-doc-links
cargo fmt --all --check
git diff --check
```

- [x] Run a changed-file LOC check and split any Rust file over 500 LOC.

- [x] Push and open the PR.

- [x] Run mandatory `lavra-review` on the PR and address all findings.

- [x] Dispatch PR review toolkit agents and address all findings.

- [x] Re-read issue #298 and audit the active PR9 checklist item-by-item:
  - `EmbeddingProvider`
  - `VectorStore`
  - `VectorPointBatch`
  - shared payload builder
  - Qdrant implementation boundary
  - vector index definitions
  - payload/redaction fixtures
  - no public CLI/MCP/REST cutover
  - no old crate deletion
  - no unvalidated new vector writes

- [ ] Post the final pre-merge gate audit to the PR/issue.

- [ ] Confirm required GitHub checks are green.

- [ ] Merge only after the audit and required checks are green.

## Expected Verification Set

Use narrow checks as development proceeds:

```bash
cargo test -p axon-embedding --locked
cargo test -p axon-vectors --locked
cargo test -p axon-retrieval --locked
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

Use broader checks only when code movement touches additional crates.
