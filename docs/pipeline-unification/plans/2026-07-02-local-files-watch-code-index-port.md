# PR11 Plan: Local Files, Watch, And Code Index Port

> **Status:** Active planning for PR11. Completed task checkboxes mark
> implemented work; final merge-gate items remain unchecked until the
> pre-merge audit, required checks, mandatory reviews, and merge actually
> complete.
>
> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

Issue: [#298](https://github.com/jmagar/axon/issues/298)
Branch: `codex/local-files-watch-code-index-port`
Base: `main` after PR10 unified jobs/observability

## Goal

Implement the planned PR11 slice from issue #298:

> **Local files + watch/code index port** — move local embed/watch/code index
> behavior onto the shared source/ledger/document/vector path.

This PR is the first source-family port. It must preserve current public local
embed/code-search/watch behavior while moving the runtime ownership model behind
that behavior toward:

```text
SourceRequest
  -> SourceResolver
  -> SourceRouter
  -> local SourceAdapter
  -> LedgerStore generation/diff/lease
  -> SourceDocument
  -> DocumentPreparer
  -> PreparedDocument
  -> EmbeddingProvider
  -> VectorPointBatch
  -> VectorStore
  -> DocumentStatus
  -> GenerationPublisher
  -> CleanupDebt
```

## Architecture

Current local indexing has two split paths:

- Generic local `embed` prepares local files through the legacy
  `axon-vector::ops::SourceDocument` bridge and writes via the legacy
  `embed_prepared_docs` TEI/Qdrant path.
- Local code search uses `axon-code-index` for SQLite project/file state,
  manifest diffing, generations, leases, cleanup debt, and filesystem watch
  refresh, while still preparing chunks through the legacy vector bridge.

The target path keeps the useful behavior but changes ownership:

- `axon-adapters::local` discovers/acquires/normalizes local files and repos and
  emits target `axon_api::source::SourceDocument` values only.
- `axon-ledger` owns local source identity, manifests, diffs, generations,
  leases, document status, publish state, and cleanup debt.
- `axon-document` prepares code, Markdown, config, structured, transcript, and
  plain text documents through `DocumentPreparer`/`ChunkRouter`.
- `axon-embedding` and `axon-vectors` own target embedding batches, vector point
  batches, payload validation, and fake/test write behavior.
- `axon-jobs` provides the source job, heartbeat, progress, cancellation,
  recovery, and provider-reservation model.
- `axon-services` is the orchestration facade. CLI/MCP/REST do not call domain
  internals directly.

`axon-code-index` may remain as compatibility/current-runtime scaffolding during
PR11, but the PR should extract or wrap the behavior that becomes target local
source behavior. Its code-index SQLite project/file store must not be the new
source of truth for the ported path.

## Tech Stack

Rust 2024, `tokio`, `async_trait`, `ignore`/Git-aware walking where already
used, `notify` watcher semantics, `sqlx` SQLite test stores, `axon-api::source`
DTOs, `axon-adapters`, `axon-ledger`, `axon-document`, `axon-embedding`,
`axon-vectors`, `axon-jobs`, `axon-observe`, `axon-services`, deterministic
fakes for provider/store boundaries.

## Global Constraints

- Use TDD: every production behavior change starts with a failing sibling test.
- Do not edit `CLAUDE.md`, `AGENTS.md`, or `GEMINI.md`.
- Keep production Rust modules under 500 LOC; split before they become dumping
  grounds.
- Use sibling `*_tests.rs` files; do not add inline `#[cfg(test)] mod tests`.
- Do not delete or hard-cut public surfaces in this PR. Public CLI/MCP/REST
  surface cutover is PR19.
- Do not port web crawl, hosted git providers, feeds, sessions, registry
  sources, CLI tools/scripts, MCP tool-call sources, memory, or reset/prune.
- Do not make old vectors searchable through the target query path. New target
  vectors use committed target generations and payloads.
- Do not use Qdrant scroll/facet results as mutable source truth.
- Do not publish a generation before required documents are prepared, embedded,
  vectorized, and document status rows are publish-safe.
- Provider unavailable before first vector write must not churn generations or
  make partial results searchable.
- Do not add source-specific payload fields outside the metadata/vector payload
  registry.
- Commit early after each task's verification passes.

## Current-State Anchors

- Existing code-index generation loop:
  `crates/axon-code-index/src/{manifest,indexer,ensure,store}.rs`
- Existing code-search service/watch:
  `crates/axon-services/src/{query,code_search_watch}.rs`
- Existing target local adapter placeholder:
  `crates/axon-adapters/src/local.rs`
- Existing target ledger boundary:
  `crates/axon-ledger/src/store.rs`
- Existing target document preparation:
  `crates/axon-document/src/{preparer,prepared,chunk_router}.rs`
- Existing target vector point builder:
  `crates/axon-vectors/src/point.rs`
- Existing target embedding provider:
  `crates/axon-embedding/src/provider.rs`
- Existing target source job model:
  `crates/axon-jobs/src/{boundary,unified}.rs`
- Contracts:
  - `docs/pipeline-unification/foundation/source-pipeline.md`
  - `docs/pipeline-unification/sources/adapter-scopes.md`
  - `docs/pipeline-unification/runtime/ledger-contract.md`
  - `docs/pipeline-unification/sources/chunking-contract.md`
  - `docs/pipeline-unification/schemas/vector-payload-schema.md`
  - `docs/pipeline-unification/runtime/job-contract.md`
  - `docs/pipeline-unification/runtime/observability-contract.md`
  - `docs/pipeline-unification/delivery/testing-contract.md`

## Non-Goals

- No removal of `embed`, `code-search`, `code-search-watch`, watch commands,
  CLI help, MCP actions, REST routes, or compatibility-era docs.
- No public `axon <source>` cutover.
- No public schema removal for old local surfaces.
- No hosted git/GitHub/GitLab/Gitea port.
- No URL watch/web crawl port.
- No live Qdrant/TEI requirement for merge-gate tests.
- No old data migration or backfill.

## Task 1: Local Adapter Capability And Manifest

**Files:**

- Modify: `crates/axon-adapters/src/local.rs`
- Modify as needed: `crates/axon-adapters/src/capability.rs`
- Modify as needed: `crates/axon-adapters/src/registry.rs`
- Test: `crates/axon-adapters/src/local_tests.rs`
- Generated docs only through schema/adapter generators if capability output
  changes.

**Interfaces:**

- Consumes: `SourcePlan`, `SourceKind::Local`, local scopes, local options.
- Produces: adapter capability, local manifest, acquired local items,
  normalized `SourceDocument` values.

- [x] Write failing tests proving the local adapter capability declares scopes:
  `file`, `directory`, `workspace`, `repo`, and `map`.
- [x] Write failing tests proving local options exist and validate:
  `include_globs`, `exclude_globs`, `respect_gitignore`, `follow_symlinks`,
  `max_file_bytes`, `binary_policy`, and `watch_policy`.
- [x] Write failing tests proving `map` returns a manifest without embedding
  documents or vector points.
- [x] Write failing tests proving local file/directory/repo discovery produces
  stable `source_item_key` values and canonical item URIs without leaking the
  absolute home path.
- [x] Implement the local adapter behind `SourceAdapter`.
- [x] Preserve current local file selection policy where possible:
  ignored/generated/cache/binary/bulk files skipped, source/docs/config files
  included, Git-aware repo walking for `.git` directories.
- [x] Run `cargo test -p axon-adapters local --locked`.

## Task 2: Local Source Pipeline Service

**Files:**

- Add/modify: `crates/axon-services/src/local_source.rs`
- Modify: `crates/axon-services/src/lib.rs`
- Test: `crates/axon-services/src/local_source_tests.rs`

**Interfaces:**

- Consumes: `SourceRequest`, `SourceAdapterRegistry`, `LedgerStore`,
  `DocumentPreparer`, `EmbeddingProvider`, `VectorStore`, `JobStore`.
- Produces: `SourceResult`, source job events, ledger generations,
  `DocumentStatus`, vector point batches, cleanup debt.

- [x] Write a failing fake-boundary test for first local file index:
  source upserted, generation created, manifest diff added, document prepared,
  embedding batch requested, vector points written, document status completed,
  generation published.
- [x] Write a failing fake-boundary test for no-change refresh:
  unchanged items reuse committed state and do not call embedding/vector writes.
- [x] Write failing fake-boundary tests for add/modify/remove:
  added/modified documents are prepared and vectorized, removed items create
  cleanup debt.
- [x] Write failing tests proving a provider failure before publish leaves the
  generation uncommitted and returns a structured degraded/failed result.
- [x] Implement the orchestration service with dependency-injected fakes first.
- [x] Ensure every detached/background run has one `job_id` and emits progress
  phases for discovering, diffing, preparing, embedding, vectorizing,
  publishing, cleaning, and complete/failed/degraded.
- [x] Run `cargo test -p axon-services local_source --locked`.

## Task 3: Target Document And Vector Payload Parity

**Files:**

- Modify as needed: `crates/axon-document/src/*`
- Modify as needed: `crates/axon-vectors/src/{payload,point}.rs`
- Test: `crates/axon-document/src/local_source_tests.rs`
- Test: `crates/axon-vectors/src/local_payload_tests.rs`

**Interfaces:**

- Consumes: local `SourceDocument` values.
- Produces: prepared code/markdown/config chunks and validated vector payloads.

- [x] Write failing tests proving local Rust files use code-aware chunking and
  carry approved `code_*` metadata where available.
- [x] Write failing tests proving local Markdown files use markdown section
  chunking with stable ranges.
- [x] Write failing tests proving manifests/config files route to structured or
  code-manifest preparation where supported.
- [x] Write failing vector payload tests for required local fields:
  `source_id`, `source_kind=local`, `source_adapter=local`, `source_scope`,
  `source_generation`, committed-generation marker, `source_item_key`,
  `item_canonical_uri`, `document_id`, `chunk_id`, `job_id`, and approved
  code/document metadata.
- [x] Implement only the missing parity gaps; do not fork a second chunker.
- [ ] Run `cargo test -p axon-document local_source --locked`.
  Existing document-preparer coverage is exercised through services in this PR;
  no new `axon-document local_source` test target was added.
- [x] Run `cargo test -p axon-vectors local_payload --locked`.

## Task 4: Code Search Compatibility Bridge

**Files:**

- Modify: `crates/axon-services/src/query.rs`
- Modify as needed: `crates/axon-code-index/src/*`
- Test: `crates/axon-services/src/query_tests.rs`
- Test as needed: `crates/axon-code-index/src/code_index_tests.rs`

**Interfaces:**

- Consumes: current `code-search`/refresh callers.
- Produces: committed-generation-only local code search behavior, structured
  stale warnings, target source pipeline refresh when enabled.

- [x] Write failing tests proving code-search searches only committed target
  generations when target local-source runtime dependencies are injected.
- [x] Write failing tests proving failed/partial refreshes stay hidden and
  return stale warnings.
- [x] Write failing tests proving current `code-search` output preserves
  untrusted-local-code semantics.
- [x] Bridge current service refresh to the target local source pipeline where
  target dependencies are injected; successful target refreshes are queryable
  through committed target source-generation filters.
- [x] Keep legacy `axon-code-index` behavior intact until target code-search
  search is feature-complete.
- [x] Run `cargo test -p axon-services query code_search --locked`.
- [ ] Run `cargo test -p axon-code-index --locked` if touched.
  Not touched in PR11.

## Task 5: Watch Refresh Integration

**Files:**

- Modify: `crates/axon-services/src/code_search_watch.rs`
- Modify as needed: `crates/axon-jobs/src/watch.rs`
- Test: `crates/axon-services/src/code_search_watch_tests.rs`
- Test as needed: `crates/axon-jobs/src/*watch*_tests.rs`

**Interfaces:**

- Consumes: local filesystem events and watch requests.
- Produces: coalesced local source refresh jobs with heartbeats/progress.

- [x] Write failing tests proving duplicate file events coalesce into one
  source refresh job.
- [x] Write failing tests proving overflow rescans schedule all watched roots
  exactly once per debounce window.
- [x] Write failing tests proving watch-triggered refreshes use unified
  `job_id`, heartbeat/progress events, provider reservations, and ledger leases
  when target local-source runtime dependencies are injected.
- [x] Preserve current foreground `embed --watch` behavior until public surface
  cutover.
- [x] Keep URL watch behavior out of this PR except shared watch/job primitives
  that local watch needs.
- [x] Run `cargo test -p axon-services code_search_watch --locked`.
- [ ] Run `cargo test -p axon-jobs watch source --locked` if touched.
  Not touched in PR11.

## Task 6: Generated Contracts, Docs, And Issue Tracker

**Files:**

- Update generated artifacts only through `cargo xtask schemas generate` or the
  relevant generator.
- Modify: `docs/pipeline-unification/delivery/current-implementation-sweep.md`
  if implementation state materially changes.
- Modify: issue #298 after PR merge.

- [x] Refresh adapter/source/vector/database/API/event artifacts if their
  source models change.
- [x] Ensure generated markdown and JSON come from the same model.
- [x] Update only docs that describe current implementation changes.
- [x] Before merge, review issue #298 and verify every PR11 checklist item.
  Local evidence now covers target committed-generation retrieval and target
  watch refresh with source jobs, progress, reservations, and ledger leases.
- [ ] After merge, update issue #298 to mark PR11 complete and record PR/merge
  commit.

## Required Local Verification

Run the smallest set that proves touched behavior, then broaden before review:

- [x] `cargo test -p axon-adapters local --locked`
- [x] `cargo test -p axon-ledger store_tests --locked`
- [ ] `cargo test -p axon-document --locked`
- [ ] `cargo test -p axon-embedding --locked`
- [x] `cargo test -p axon-vectors --lib --locked`
- [x] `cargo test -p axon-vectors store_tests --lib`
- [ ] `cargo test -p axon-jobs watch source --locked`
- [x] `cargo test -p axon-services local_source --locked`
- [x] `cargo test -p axon-services code_search --locked`
- [x] `cargo test -p axon-services code_search_watch::tests --lib`
- [x] `cargo test -p axon-services --lib --locked`
- [ ] `cargo test -p axon-vector file_ingest source_doc code_search --locked` if
  compatibility bridges are touched
- [ ] `cargo test -p axon-code-index --locked` if compatibility scaffolding is
  touched
- [x] `cargo xtask schemas generate --check`
- [x] `cargo xtask check-layering`
- [x] `cargo xtask check-repo-structure`
- [x] `cargo xtask check-doc-links`
- [x] `cargo xtask check-doc-contracts`
- [x] `cargo fmt --all --check`
- [x] `git diff --check`

## Mandatory Reviews Before Merge

- [ ] Run `lavra:lavra-review` over the PR and address all issues.
- [ ] Dispatch PR Review Toolkit agents over the entire PR and address all
  issues.
- [ ] Confirm CI is green.
- [ ] Confirm issue #298 PR11 checklist is fully implemented.
- [ ] Merge only after checks and reviews are complete.
