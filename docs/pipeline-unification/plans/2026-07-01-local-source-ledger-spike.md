# Local Source Ledger Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Create PR2 for issue #298 by proving one local source can flow through the target pipeline shape far enough to expose the real friction before public CLI/MCP/REST rewiring.

**Target flow:** `SourceRequest(local path) -> local adapter prototype -> SourceLedger draft/generation -> SourceDocument -> prepare_source_document -> PreparedDoc`

**Architecture:** This PR is a spike, not a public cutover. It keeps the current `embed`, `code-search`, watch, Qdrant, TEI, CLI, MCP, and REST behavior unchanged. The spike lives behind an internal services module and tests so we can prove ledger ownership enters before document preparation.

**Tech Stack:** Rust 2024, `axon-services`, `axon-source-ledger`, `axon-vector`, SQLite source-ledger store, existing `SourceDocument` and `PreparedDoc` runtime types.

## Global Constraints

- Do not wire this path to public CLI, MCP, REST, or watch commands.
- Do not remove, rename, or alias existing commands in this PR.
- Do not publish generations through the new path.
- Do not write Qdrant points through the new path.
- Do not rewrite the job runtime.
- Do not migrate, tombstone, or preserve existing local data.
- Do not persist cleanup debt in the spike; persistent cleanup debt belongs to the later ledger-owned lifecycle PR.
- Do not touch any `CLAUDE.md` files.
- Keep files under 500 lines.
- Add sibling test files; do not add inline tests in implementation modules.
- Prefer existing source-ledger and vector preparation APIs over inventing parallel abstractions.
- Commit the plan before implementation; commit implementation after the targeted tests pass.

## Target PR Scope

This PR includes:

- An internal local-source spike module in `axon-services`.
- A small local-source adapter prototype that discovers local files and emits manifest/source-document inputs.
- A source-ledger draft generation for the local source before preparation.
- Markdown and Rust fixture tests proving prepared docs carry canonical local source and draft generation metadata.
- Ledger assertions for source id, source kind, collection, generation, item keys, and content hashes.
- Output-only cleanup placeholders proving the shape that later PRs will persist as ledger cleanup debt.
- Spike notes recording which current local embed/watch pieces should move versus be rewritten in later PRs.

This PR excludes:

- CLI command changes.
- MCP tool schema changes.
- REST route changes.
- Watch scheduler changes.
- Background job runtime changes.
- Qdrant collection or payload writes.
- Cleanup debt persistence or execution.
- Public config changes.

## Implementation Shape

Add an internal services module:

```text
crates/axon-services/src/source_spike.rs
crates/axon-services/src/source_spike_tests.rs
```

Expose the module only from `crates/axon-services/src/lib.rs` for crate tests or as a clearly experimental internal API. It must not be called by CLI/MCP/REST.

Core structs:

```rust
pub(crate) struct LocalSourceSpikeInput {
    pub root: PathBuf,
    pub collection: String,
    pub owner: String,
}

pub(crate) struct LocalSourceSpikeOutput {
    pub source_id: String,
    pub source_kind: String,
    pub collection: String,
    pub generation: i64,
    pub manifest_items: Vec<LocalSourceSpikeManifestItem>,
    pub prepared_docs: Vec<PreparedDoc>,
    pub cleanup_placeholders: Vec<LocalSourceSpikeCleanupPlaceholder>,
}
```

Use concrete field types from the existing crates where ergonomic. The spike output may stay small, but it must expose enough state for tests to prove the lifecycle.

## Task 1: Add Failing Spike Tests

- [x] Create `crates/axon-services/src/source_spike_tests.rs`.
- [x] Add a Markdown fixture test:
  - create a temp local source root with `README.md`
  - create an isolated SQLite source-ledger store
  - run the spike
  - assert one draft generation exists
  - assert the manifest contains `README.md`
  - assert the manifest item has a stable content hash and nonzero size
  - assert at least one prepared doc/chunk is produced
  - assert prepared metadata contains the local source id and draft generation
- [x] Add a Rust fixture test:
  - create a temp local source root with `src/lib.rs`
  - run the spike
  - assert the item key is `src/lib.rs`
  - assert prepared output uses the code-aware preparation path
  - assert prepared metadata contains the local source id and draft generation
- [x] Add a ledger status assertion:
  - source kind is local path/local filesystem
  - collection is the requested collection
  - committed generation is still unset/zero
  - active or max generation matches the spike generation
- [x] Add cleanup placeholder assertions:
  - placeholders identify the source and generation
  - placeholders are not executed
  - placeholders make clear later PRs will persist cleanup into ledger cleanup debt

Run and confirm the new tests fail for missing implementation:

```bash
cargo test -p axon-services source_spike --locked
```

## Task 2: Implement Local Source Manifest Discovery

- [x] Add `crates/axon-services/src/source_spike.rs`.
- [x] Resolve `LocalSourceSpikeInput.root` to a canonical local root where possible.
- [x] Support a single file or directory.
- [x] Walk directories deterministically.
- [x] Ignore directories/files already ignored by the existing local embed path where a reusable helper exists.
- [x] Produce stable relative item keys using slash separators.
- [x] Compute content hash, size, and modified timestamp for each file.
- [x] Infer content kind/language using the existing source-document or input-classification helpers where possible.
- [x] Keep acquisition local-only and synchronous; no network or browser behavior.

## Task 3: Create Ledger Draft Generation Before Preparation

- [x] Build a `SourceIdentity` for the local root.
- [x] Ensure the source exists in `SourceLedgerStore`.
- [x] Begin a new generation owned by the spike owner.
- [x] Diff or record the discovered manifest through existing ledger APIs.
- [x] Do not commit the generation.
- [x] Do not publish the generation.
- [x] Abort or release any lease/generation cleanly on test failure paths if the store API requires it.
- [x] Return ledger-derived source id, source kind, collection, generation, and manifest details in `LocalSourceSpikeOutput`.

## Task 4: Convert Local Items To SourceDocument And PreparedDoc

- [x] Convert each manifest item into the existing runtime `axon_vector::ops::source_doc::SourceDocument`.
- [x] Attach ledger metadata using the sanctioned ledger payload path, not spoofable raw extras.
- [x] Include source id, source kind, collection, generation, item key, canonical URI, file path, content hash, size, and content kind in metadata.
- [x] Call the existing `prepare_source_document` path for each document.
- [x] Keep `prepare_embed_docs` unchanged.
- [x] Keep `PreparedDoc` shape unchanged unless the tests prove a minimal metadata hook is missing.

## Task 5: Record Move-Versus-Rewrite Notes

- [x] Add `docs/pipeline-unification/delivery/local-source-ledger-spike-notes.md`.
- [x] Document which current local embed/watch pieces can move mostly as-is:
  - file discovery/exclusion
  - source-document construction
  - code/markdown preparation
  - TEI batching knobs
- [x] Document which pieces should be rewritten in later PRs:
  - public command surface
  - watch ownership/status naming
  - generation publish
  - cleanup debt execution
  - progress/heartbeat model
- [x] Link the notes back to issue #298 and this PR2 plan.

## Task 6: Verify

- [x] Run targeted services tests:

```bash
cargo test -p axon-services source_spike --locked
```

- [x] Run source-ledger tests if ledger APIs were touched:

```bash
cargo test -p axon-source-ledger --locked
```

- [x] Run vector source-doc tests if source-document metadata handling was touched:

```bash
cargo test -p axon-vector source_doc --locked
```

- [x] Run formatting:

```bash
cargo fmt -- --check
```

- [x] Run the lightweight repo structure check:

```bash
cargo xtask check-repo-structure
```

## Done Criteria

- Local Markdown fixture reaches `PreparedDoc` through a ledger draft generation.
- Local Rust fixture reaches `PreparedDoc` through a ledger draft generation.
- Tests prove the ledger draft exists before preparation.
- Tests prove generation is not committed/published by the spike.
- Current public `embed`, `code-search`, watch, CLI, MCP, and REST behavior is unchanged.
- Move-versus-rewrite notes exist for the later full implementation.
