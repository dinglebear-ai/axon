# LLM-Format Epic Closeout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close out the LLM-optimized format epic (axon_rust-zzre / axon_rust-y34v) by running the validation pass, closing the stale-open parent beads, and filing two follow-up beads for the genuinely deferred scope.

**Architecture:** The core `to_llm_text()` transform and all three wiring points (CLI `select_output`, MCP schema, MCP map helpers) are already implemented. What remains is: (1) confirming the tests and binary compile cleanly, (2) doing a quick smoke-test against a real URL, (3) closing the parent/wrapper beads, and (4) creating the two deferred-scope beads so the follow-up work is not lost.

**Tech Stack:** Rust / Cargo, `rtk` wrapper for token-efficient output, `bd` (beads) issue tracker, `axon` binary with `--format llm` flag, pulldown-cmark (already in use via transitive dep).

---

## Context: What is already done

All implementation work is complete and merged to `feature/gitlab-ingest`:

| File | Status |
|---|---|
| `src/core/content/llm.rs` | Done — `to_llm_text()` transform |
| `src/core/content/llm_tests.rs` | Done — sidecar test file |
| `src/crawl/scrape.rs` `select_output()` | Done — `ScrapeFormat::Llm` arm |
| `src/mcp/schema/requests.rs` | Done — `McpScrapeFormat::Llm` variant |
| `src/mcp/server/common.rs` | Done — `map_scrape_format` arm |
| `src/services/action_api/commands/helpers.rs` | Done — second `map_scrape_format` arm |
| Beads zzre.1, zzre.2, zzre.3 | All closed (✓) |

## Context: What is NOT done

| Item | Status |
|---|---|
| `axon_rust-zzre` parent bead | Open — needs closing after validation |
| `axon_rust-y34v` epic wrapper | Open — needs closing after zzre closes |
| `axon_rust-lrou` swarm molecule | Open — needs closing after y34v closes |
| Vertical extractor LLM format bypass | Known v1 limitation — needs follow-up bead |
| Crawl streaming (`axon crawl --format llm`) | Deferred — needs follow-up bead |

## Known Limitations (v1 decisions)

### Vertical bypass (src/services/scrape.rs ~line 97)

When `cfg.enable_verticals = true` and a URL matches a registered vertical extractor (github_repo, pypi, npm, reddit, etc.), the extractor result is returned directly without applying `select_output()`. The comment reads:

```rust
// v1: LLM format is only applied on the generic HTTP scrape path.
// Vertical extractors return structured markdown that should not be post-processed.
return Ok(scrape_result);
```

**Implication:** `axon scrape --format llm https://github.com/rust-lang/rust` will NOT produce LLM-formatted output because the github_repo extractor claims the URL first. This is intentional for v1 — vertical extractor output is already structured markdown from API data, and applying the LLM transform on top adds noise rather than signal.

### Crawl streaming deferred

`axon crawl --format llm` does not apply LLM format because the crawl collector writes raw markdown files to disk during crawl and the `cfg.format` field is never read by the engine. A post-crawl read-back pass would be needed. This was explicitly deferred in the zzre bead spec.

---

## File Map

No new files are created by this plan. The only file edits are bead state changes via `bd`. The two new beads are created as new issues.

---

## Task 1: Validation Pass

**Files:** No file edits. Read-only verification.

- [ ] **Step 1.1: Run LLM-specific tests**

```bash
rtk cargo test -q llm
```

Expected output: `cargo test: 53 passed, 2216 filtered out (21 suites, 1.00s)` (or similar — no failures).

The 53 tests include all tests whose names contain "llm". If any fail, do NOT proceed — investigate the failure first.

- [ ] **Step 1.2: Run full cargo check**

```bash
rtk cargo check --bin axon
```

Expected: `cargo build: 0 errors` (warnings are acceptable — there are currently 3 pre-existing warnings about unnecessary path qualifications in `src/core/config/types/subconfigs.rs`).

- [ ] **Step 1.3: Build the release binary**

```bash
rtk cargo build --release --bin axon 2>&1 | tail -5
```

Expected: build completes successfully. This confirms the binary is linkable, not just type-checkable.

- [ ] **Step 1.4: Smoke-test with a real URL**

```bash
./target/release/axon scrape --format llm https://example.com 2>/dev/null | head -20
```

Expected output begins with the URL metadata header and shows clean prose without markdown emphasis markers:

```
> URL: https://example.com

# Example Domain

This domain is for use in illustrative examples...
```

Expected: no `**bold**` or `*italic*` markers in body text. A `## Links` section at the end if any links were found.

- [ ] **Step 1.5: Verify format flag is visible in help**

```bash
./target/release/axon scrape --help | grep -A3 "format"
```

Expected: `llm` appears as a valid value for `--format`.

---

## Task 2: Close Stale-Open Beads

**Files:** No file edits. Only `bd close` commands.

Close in dependency order: child (zzre) → epic (y34v) → molecule (lrou).

- [ ] **Step 2.1: Close the zzre parent bead**

All three children (zzre.1, zzre.2, zzre.3) are already closed. Close the parent:

```bash
bd close axon_rust-zzre
```

When prompted for a comment, enter:

```
Validation pass passed: `cargo test -q llm` (53 tests green), `cargo check --bin axon` (0 errors), smoke-test of `axon scrape --format llm https://example.com` produces LLM-formatted output with URL header and cleaned body. All three child beads (zzre.1, zzre.2, zzre.3) were closed in the implementation session. Two follow-up beads filed: axon_rust-XXXX (vertical LLM bypass) and axon_rust-YYYY (crawl streaming).
```

- [ ] **Step 2.2: Close the y34v epic wrapper**

```bash
bd close axon_rust-y34v
```

When prompted for a comment, enter:

```
Epic complete. zzre closed after successful validation pass. Two deferred items tracked as separate beads.
```

- [ ] **Step 2.3: Close the lrou swarm molecule**

```bash
bd close axon_rust-lrou
```

When prompted for a comment, enter:

```
Molecule closed. Coordinated epic axon_rust-y34v is now done.
```

- [ ] **Step 2.4: Verify all three are closed**

```bash
bd list 2>&1 | grep -E "zzre|y34v|lrou"
```

Expected: all three show `✓` status. If any still show `○`, re-run the close command for that bead.

---

## Task 3: File Follow-Up Bead A — Vertical Extractor LLM Format

**Files:** No code changes. New bead created via `bd create`.

This bead tracks applying LLM format post-processing to vertical extractor output when `--format llm` is requested. Currently the vertical path returns early before `select_output()` is called.

- [ ] **Step 3.1: Create the bead**

```bash
bd create
```

Fill in the prompts as follows:

**Title:** `Apply --format llm to vertical extractor output path`

**Type:** `feature`

**Priority:** `P3`

**Description:**

```markdown
## Why this issue exists

`axon scrape --format llm` currently bypasses LLM formatting for URLs handled by vertical extractors (github_repo, pypi, npm, reddit, crates_io, etc.). The vertical path in `src/services/scrape.rs` returns early before `select_output()` applies the `ScrapeFormat::Llm` transform.

This is a v1 decision documented with a comment: "Vertical extractors return structured markdown that should not be post-processed." In practice, users who request `--format llm` on a GitHub URL get standard vertical extractor markdown instead of the LLM-optimized form they asked for.

## Scope

Modify `src/services/scrape.rs` in the vertical fast-path `Ok(Some(result))` arm to apply `to_llm_text()` when `cfg.format == ScrapeFormat::Llm`.

The code change is at `src/services/scrape.rs` around line 100:

```rust
// Current (v1):
return Ok(scrape_result);

// v2 target:
if cfg.format == ScrapeFormat::Llm {
    scrape_result.output = crate::core::content::to_llm_text(&scrape_result.output, &normalized);
}
return Ok(scrape_result);
```

Note: `scrape_result.output` is set to the raw `markdown` from the vertical doc in `map_scrape_payload`. Applying `to_llm_text()` to it will clean up bold/italic emphasis and aggregate links — but vertical extractors already produce clean structured markdown, so the impact is minimal.

## Acceptance criteria

- `axon scrape --format llm https://github.com/rust-lang/rust` produces output with URL header and no bold/italic markers
- `axon scrape --format llm https://crates.io/crates/serde` same
- Existing vertical extractor output (without `--format llm`) is unaffected
- Tests: add test in `src/services/scrape_tests.rs` (or new sidecar) that mocks a vertical extractor result and asserts LLM format is applied

## Files to touch

- `src/services/scrape.rs` — add LLM post-processing in vertical fast-path arm
- `src/services/scrape_tests.rs` (or create if missing) — add test for vertical + llm format
```

- [ ] **Step 3.2: Verify bead was created**

```bash
bd list 2>&1 | tail -5
```

Note the new bead ID for reference in commit messages.

---

## Task 4: File Follow-Up Bead B — Crawl Streaming LLM Format

**Files:** No code changes. New bead created via `bd create`.

This bead tracks `axon crawl --format llm` applying LLM formatting to each crawled page's output, either at collection time or via a post-crawl read-back pass.

- [ ] **Step 4.1: Create the bead**

```bash
bd create
```

Fill in the prompts as follows:

**Title:** `axon crawl --format llm: post-crawl read-back pass`

**Type:** `feature`

**Priority:** `P3`

**Description:**

```markdown
## Why this issue exists

`axon crawl --format llm` currently produces standard markdown output. The crawl collector (`src/crawl/engine.rs`) writes raw markdown files to disk during crawl and never reads `cfg.format`. The `ScrapeFormat` field in `Config` is not propagated into the collector pipeline.

This was explicitly deferred in axon_rust-zzre: "Crawl streaming (`axon crawl --format llm --wait true > docs.txt`) is deferred to a follow-up bead — the crawl collector does not read `cfg.format` and streaming requires a separate post-crawl read-back phase."

## Design options

Two approaches:

**Option A — Collector inline transform (preferred for clean implementation)**
Pass `cfg.format` into the collector. When `format == ScrapeFormat::Llm`, apply `to_llm_text()` to each page's markdown before writing it to disk. Requires threading `format` through:
- `CollectorConfig` in `src/crawl/engine.rs`
- The page handler closure inside `collect_pages()`

**Option B — Post-crawl read-back pass (simpler, no collector change)**
After the crawl completes, scan the manifest and re-read each markdown file from `output_dir/markdown/`, apply `to_llm_text()`, and write it back in-place. Triggered by `cfg.format == ScrapeFormat::Llm` in the crawl runner (`src/jobs/workers/runners/crawl.rs`).

Option A is cleaner (no double-write, no extra I/O pass) but touches the Spider collector closure. Option B is safer (isolated post-processing step, no concurrency concerns) but doubles disk I/O.

## Acceptance criteria

- `axon crawl --format llm --wait true https://docs.rs/serde` produces an `output_dir/markdown/` tree where each file has the URL metadata header and no bold/italic markers
- `axon crawl` without `--format llm` is unaffected
- Existing embed pipeline (which reads from `output_dir/markdown/`) still works after LLM post-processing is applied
- MCP `crawl` action with `format: "llm"` also applies the transform

## Key files

- `src/crawl/engine.rs` — collector pipeline (Option A) — `collect_pages()` closure
- `src/jobs/workers/runners/crawl.rs` — post-crawl hook point (Option B)
- `src/core/content/llm.rs` — `to_llm_text()` is already implemented, just needs to be called
- `src/jobs/config_snapshot.rs` — `format: Option<ScrapeFormat>` field already stored in job config snapshot, so the format value survives job serialization/deserialization
```

- [ ] **Step 4.2: Verify bead was created**

```bash
bd list 2>&1 | tail -5
```

Note the new bead ID.

---

## Task 5: Commit and Push

- [ ] **Step 5.1: Check git status**

```bash
rtk git status
```

Expected: clean or only plan file additions. No implementation files should have uncommitted changes.

- [ ] **Step 5.2: Stage the plan file**

```bash
rtk git add docs/superpowers/plans/2026-05-21-llm-format-epic-closeout.md
```

- [ ] **Step 5.3: Commit**

```bash
rtk git commit -m "$(cat <<'EOF'
docs: add llm-format epic closeout plan + validation checklist

Closes axon_rust-zzre / axon_rust-y34v / axon_rust-lrou after
validation pass. Documents v1 vertical bypass limitation and
crawl streaming deferral as follow-up bead scope.
EOF
)"
```

- [ ] **Step 5.4: Push**

```bash
rtk git push
```

- [ ] **Step 5.5: Verify push succeeded**

```bash
rtk git status
```

Expected: `Your branch is up to date with 'origin/feature/gitlab-ingest'`.

---

## Self-Review

### Spec coverage check

| Requirement | Task |
|---|---|
| Confirm tests pass | Task 1.1 |
| Confirm cargo check passes | Task 1.2 |
| Manual smoke-test | Task 1.4 |
| Close zzre parent bead | Task 2.1 |
| Close y34v epic | Task 2.2 |
| Close lrou molecule | Task 2.3 |
| File vertical bypass follow-up bead | Task 3 |
| File crawl streaming follow-up bead | Task 4 |
| Document v1 limitations | Tasks 3 and 4 descriptions |
| Push to remote | Task 5 |

### Placeholder scan

No TBD or TODO placeholders. All commands are concrete with expected output. All bead descriptions include exact file references and code snippets.

### Type consistency

No new types defined. All references to `ScrapeFormat::Llm`, `to_llm_text()`, `select_output()`, and `cfg.format` match the actual code at the time this plan was written (verified against codebase).

---

## Execution Handoff

**Plan saved to `docs/superpowers/plans/2026-05-21-llm-format-epic-closeout.md`.** Two execution options:

**1. Subagent-Driven (recommended)** — Fresh subagent per task, review between tasks, fast iteration. Use `superpowers:subagent-driven-development`.

**2. Inline Execution** — Execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints.

Which approach?
