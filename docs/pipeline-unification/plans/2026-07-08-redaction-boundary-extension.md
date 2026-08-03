# Redaction Boundary Extension Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close Phase 3B Task 4's remaining gap — extend the existing redaction boundary (already gating vector payloads, job events, graph evidence, memory rows, and MCP/REST error responses) to CLI JSON output, artifact metadata writes, and trace/log fields crossing public visibility.

**Architecture:** Three independent gate-insertion points, each following the identical shape: find the write/render call site, prove a secret-bearing fixture is caught before it, wire the existing shared redaction function in. Split into three separate committed steps (not bundled into one task) per engineering review, since each is independently sized and none depends on the others — this also means an incomplete implementer can land 1-of-3 rather than being blocked on all three landing atomically.

**Tech Stack:** Rust 2024, `axon-core`, `axon-cli`.

## Global Constraints

- Split out of `2026-07-08-finish-job-cutover-and-security-completion.md` per engineering review: independent of the job cutover.
- Redaction failure fails closed: never call the downstream writer/render function after a redaction failure.
- This plan's per-surface manual grep is explicitly a best-effort pass, not an exhaustive guarantee — Task 4 below files a tracked follow-up for CI/lint enforcement rather than claiming completeness the grep can't back.
- Do not edit `CLAUDE.md`, `AGENTS.md`, or `GEMINI.md`.
- Commit after each task's verification passes.

---

## Source-Of-Truth Contracts

- `docs/pipeline-unification/plans/2026-07-04-phase-3b-security-error-memory-completion.md` (Task 4)
- `docs/pipeline-unification/runtime/security-contract.md`

## Current-State Anchors

- Existing shared gate (already covers vector/job-event/graph/memory/MCP/REST-error paths): `crates/axon-core/src/redact.rs` + `crates/axon-core/src/redact/boundary.rs`. Read this file first for the real function name — it does not match the illustrative names (`redact_public_write`) used in the originating plan; confirm the real name before writing any task against it.
- CLI JSON output: the shared `--json` render path referenced in this repo's root `CLAUDE.md` ("`--json` flag enables machine-readable output on all commands"). Find it under `crates/axon-cli/src/` — likely `json.rs` or similar, per `axon-cli`'s module map, but confirm by reading, don't assume the filename.
- Artifact metadata writes: `ArtifactStore` public metadata write path, per `docs/pipeline-unification/runtime/storage-contract.md`'s `ArtifactStore Ownership` table (visibility/content_hash/relative_path/etc.).
- Trace/log fields: `axon_core::logging`/`tracing::*!` call sites.

## File Structure

- Modify: `crates/axon-core/src/redact/boundary.rs`
- Modify: CLI JSON render helper (path confirmed in Task 1)
- Modify: artifact metadata write path
- Modify: identified trace/log call sites crossing public visibility
- Test: `crates/axon-core/src/redact/boundary_tests.rs`

---

## Task 1: Read The Real Gate Function And Locate The Three Chokepoints

**Files:**
- Read only: `crates/axon-core/src/redact/boundary.rs`, `crates/axon-cli/src/**`, artifact-write module, logging module

**Interfaces:**
- Consumes: nothing — research step.
- Produces: confirmed real gate function name/signature and confirmed real file:line for each of the three chokepoints.

- [ ] **Step 1: Read the existing gate**

Read `crates/axon-core/src/redact/boundary.rs` in full. Record the real function name (not `redact_public_write`), its parameter shape (surface enum, value, context), and its return type (does it return `Result<RedactedValue, ApiError>` or something else — confirm exactly).

- [ ] **Step 2: Find the CLI JSON chokepoint**

Read `crates/axon-cli`'s module map (its `CLAUDE.md`) and locate the single shared function every `--json`-flagged command routes through before writing to stdout. Confirm it's genuinely shared (one function) and not duplicated per-command — if it's duplicated, note that as a larger finding requiring its own remediation before this task's fix can be applied in one place.

- [ ] **Step 3: Find the artifact metadata chokepoint**

Locate where `ArtifactStore` writes its public metadata row/fields (visibility, content_hash, relative_path, etc.).

- [ ] **Step 4: Find candidate trace/log call sites**

Grep for `tracing::info!`/`tracing::warn!`/`log_info`/`log_warn` call sites that log a value later exposed at `Visibility::Public` (e.g. a raw URL, a memory body snippet, a user-supplied query string). List every candidate found — this list becomes Task 4's scope.

## Task 2: Gate CLI JSON Output

**Files:**
- Modify: the CLI JSON render helper found in Task 1 Step 2
- Test: `crates/axon-core/src/redact/boundary_tests.rs`, `crates/axon-cli/src/json_tests.rs` (or wherever CLI JSON tests already live)

**Interfaces:**
- Consumes: Task 1's confirmed real gate function.
- Produces: untrusted-mode `--json` renders pass through the gate before writing to stdout.

- [ ] **Step 1: Write a failing fail-closed test**

Using the existing vector-payload fail-closed test in `boundary_tests.rs` as the template for fixture shape and assertion style (do not invent a new fixture format), add a CLI-JSON-surface variant:

```rust
#[test]
fn cli_json_output_secret_fixture_fails_before_render() {
    let payload = secret_payload_fixture();
    let err = /* real gate function name from Task 1 */(RedactionSurface::CliJson, payload).unwrap_err();
    assert_eq!(err.code.to_string(), "redaction.failed");
}
```

- [ ] **Step 2: Run test and confirm failure**

Run: `cargo test -p axon-core cli_json_output_secret_fixture_fails_before_render --no-fail-fast`

Expected: FAIL — `RedactionSurface::CliJson` likely doesn't exist yet as a variant, or the CLI render path doesn't call the gate.

- [ ] **Step 3: Add the surface variant and wire the gate**

Add `RedactionSurface::CliJson` if missing, and call the gate function from the CLI JSON render helper before writing to stdout, for untrusted-mode renders (check whether this repo distinguishes trusted-local vs untrusted CLI invocation contexts — if it doesn't, gate all `--json` output unconditionally, since a wrong-but-safe default is preferable to a right-but-unenforced one).

- [ ] **Step 4: Run test**

Run: `cargo test -p axon-core cli_json_output_secret_fixture_fails_before_render --no-fail-fast`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/axon-core/src crates/axon-cli/src
git commit -m "feat(security): gate CLI JSON output through the redaction boundary"
```

## Task 3: Gate Artifact Metadata Writes

**Files:**
- Modify: the artifact-write module found in Task 1 Step 3
- Test: `crates/axon-core/src/redact/boundary_tests.rs`

**Interfaces:**
- Consumes: Task 1's confirmed real gate function and artifact-write chokepoint.
- Produces: `ArtifactStore` public metadata writes pass through the gate before the row is written.

- [ ] **Step 1: Write a failing fail-closed test**

```rust
#[test]
fn artifact_metadata_secret_fixture_fails_before_write() {
    let metadata = secret_artifact_metadata_fixture();
    let err = /* real gate function */(RedactionSurface::Artifact, metadata).unwrap_err();
    assert_eq!(err.code.to_string(), "redaction.failed");
}
```

- [ ] **Step 2: Run test and confirm failure**

Run: `cargo test -p axon-core artifact_metadata_secret_fixture_fails_before_write --no-fail-fast`

Expected: FAIL.

- [ ] **Step 3: Add the surface variant and wire the gate**

Add `RedactionSurface::Artifact` if missing, call the gate in the artifact-write chokepoint before the metadata row is persisted.

- [ ] **Step 4: Run test**

Run: `cargo test -p axon-core artifact_metadata_secret_fixture_fails_before_write --no-fail-fast`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/axon-core/src
git commit -m "feat(security): gate artifact metadata writes through the redaction boundary"
```

## Task 4: Best-Effort Gate Trace/Log Call Sites, File A Follow-Up For Exhaustive Coverage

**Files:**
- Modify: the call sites identified in Task 1 Step 4
- Test: `crates/axon-core/src/redact/boundary_tests.rs`

**Interfaces:**
- Consumes: Task 1's candidate list.
- Produces: every candidate call site from Task 1 Step 4 gated; an explicit tracked follow-up (bd issue or a note in the source-of-truth plan doc) for a lint/CI mechanism, since this task's manual grep is not a completeness guarantee.

- [ ] **Step 1: Write one fail-closed test per identified call site**

For each call site found in Task 1 Step 4, add a targeted test proving a secret-bearing input to that specific function no longer reaches the log sink unredacted (use a test log subscriber/capture, matching whatever pattern this repo's existing logging tests already use).

- [ ] **Step 2: Run tests and confirm failure**

Run the new tests; expect FAIL for each call site not yet gated.

- [ ] **Step 3: Wire each call site through the gate**

For each, route the logged value through the gate function before the `tracing::*!`/`log_*` call, or redact inline if the gate function's surface shape doesn't fit a bare string (in which case, use whatever lower-level redaction primitive the gate function itself is built on — read `boundary.rs`'s implementation to find it rather than reimplementing detection logic).

- [ ] **Step 4: Run tests**

Expected: PASS for every call site gated in Step 3.

- [ ] **Step 5: File the exhaustiveness follow-up**

This repo uses `bd` for issue tracking (see root `CLAUDE.md`). Run `bd create --title="Add CI-enforced redaction call-site lint" --description="Task 4 of docs/pipeline-unification/plans/2026-07-08-redaction-boundary-extension.md gated trace/log call sites found by a manual grep, which is not exhaustive. Add a lint (clippy custom lint, or a grep-based xtask check) that fails CI when a new tracing::*!/log_* call site logs a value tagged as potentially-secret without routing through the redaction gate." --type=task --priority=2`.

- [ ] **Step 6: Commit**

```bash
git add crates/axon-core/src crates/axon-services/src
git commit -m "feat(security): gate identified trace/log call sites through the redaction boundary"
```

## Task 5: Verification

- [ ] **Step 1: Full crate gate**

```bash
cargo test -p axon-core redact --no-fail-fast
cargo test -p axon-cli --no-fail-fast
cargo clippy -p axon-core -p axon-cli --all-targets
```

Expected: PASS.

- [ ] **Step 2: Update the source plan doc**

Mark Phase 3B Task 4 done (with the exhaustiveness caveat noted) in `2026-07-04-phase-3b-security-error-memory-completion.md`.

- [ ] **Step 3: Commit**

```bash
git add docs/pipeline-unification/plans
git commit -m "docs(pipeline): close out phase 3b task 4 redaction boundary extension"
```

## Self-Review

- Spec coverage: Phase 3B Task 4 → Tasks 1-4 here.
- Engineering review findings applied: task split into 3 independently-committable steps instead of one bundled task (matches this repo's own "commit after each task's verification passes" constraint, which the original bundled version violated in practice); exhaustiveness explicitly not claimed — a tracked follow-up replaces a false completeness signal.
- Placeholder scan: Task 1 is an explicit read-first step for the same reason as the provider-cooling plan's Task 1 — a prior draft guessed at function/file names that turned out wrong.
