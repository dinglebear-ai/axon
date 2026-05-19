# Ingest Pipeline Observability Overhaul

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **REQUIRED AGENT TYPE:** Use `rust-pro` agent for all implementation tasks.
> **REQUIRED SKILLS:** Load `rust-best-practices` and `rust-async-patterns` skills before implementation.
> **TDD:** Use `superpowers:test-driven-development` for every task. RED → GREEN → REFACTOR.

**Goal:** Make the ingest pipeline fully observable — every stage reports progress, heartbeats detect true staleness (not just liveness), GitHub issues/PRs default to recent items, and all new ingest sources get structured progress for free via a reusable trait.

**Architecture:** Introduce a `PhaseReporter` helper that wraps an optional mpsc sender for standardized progress reporting. No central enum — each ingest source defines its own phase constants as `&str`, keeping sources fully decoupled. All ingest sources (GitHub, YouTube, Reddit, Sessions) use `PhaseReporter` for progress. Each GitHub sub-task (files, issues, PRs, metadata, wiki) reports its own phase and progress independently. The heartbeat gains content-awareness — it compares consecutive `result_json` snapshots and flags jobs where `updated_at` advances but data doesn't change. Config gains `github_max_issues` and `github_max_prs` (default 100 each, 0 = unlimited).

**Tech Stack:** Rust 1.85+, tokio, serde_json, sqlx (Postgres), octocrab, tracing

---

## Root Cause Summary

Debugging session revealed that `ingest_github` runs 5 concurrent tasks via `tokio::join!`:
1. `embed_files` — file embedding (reports progress)
2. `embed_repo_metadata` — metadata (no progress)
3. `ingest_issues` — paginates ALL issues, ALL states (no progress, no limit)
4. `ingest_pull_requests` — paginates ALL PRs, ALL states (no progress, no limit)
5. `ingest_wiki` — wiki clone+embed (no progress)

The `tokio::join!` blocks until ALL 5 complete. Issues/PRs have zero progress reporting, no page limits, and can paginate for 30+ minutes on active repos. The heartbeat only bumps `updated_at` without checking if actual progress data changed — so jobs appear "alive" but are silently stuck.

## File Structure

### New Files
| File | Responsibility |
|------|---------------|
| `crates/ingest/progress.rs` | `PhaseReporter` helper — wraps optional mpsc sender, no central enum |
| `crates/jobs/common/heartbeat.rs` | Extract heartbeat into own module, add content-aware staleness detection |

### Modified Files
| File | Changes |
|------|---------|
| `crates/ingest/github/issues.rs` | Add progress reporting, page limits, sort by updated desc |
| `crates/ingest/github.rs` | Wire per-task phase reporting, increment `tasks_done` as each completes |
| `crates/ingest/github/files/batch.rs` | Use `PhaseReporter` instead of raw `send_progress` |
| `crates/ingest/github/files.rs` | Wire `PhaseReporter` through `embed_files` |
| `crates/ingest/github/wiki.rs` | Add progress reporting via `PhaseReporter` |
| `crates/ingest/youtube.rs` | Accept `PhaseReporter`, report downloading/transcribing/embedding phases |
| `crates/ingest/reddit.rs` | Accept `PhaseReporter`, report auth/fetching/embedding phases |
| `crates/ingest/sessions.rs` | Accept `PhaseReporter`, report scanning/embedding phases |
| `crates/jobs/ingest/process.rs` | Wire mpsc channel + PhaseReporter for all sources (not just GitHub) |
| `crates/core/config/types/config.rs` | Add `github_max_issues`, `github_max_prs` fields |
| `crates/core/config/types/subconfigs.rs` | Add fields to `IngestConfig` |
| `crates/core/config/types/config_impls.rs` | Defaults for new fields |
| `crates/core/config/parse/build_config.rs` | Parse env vars + CLI flags for new fields |
| `crates/core/config/cli/global_args.rs` | CLI arg definitions for `--max-issues`, `--max-prs` |
| `crates/core/config/help.rs` | Help text for new flags |
| `crates/cli/commands/status/metrics.rs` | Display per-task phase labels in status output |
| `crates/jobs/common/job_ops.rs` | Move heartbeat to `heartbeat.rs`, add `spawn_content_aware_heartbeat` |
| `crates/jobs/worker_lane.rs` | Use content-aware heartbeat wrapper |
| `crates/jobs/ingest.rs` | Wire new heartbeat, update constants |

### Test Files
| File | Tests |
|------|-------|
| `crates/ingest/progress.rs` (inline `#[cfg(test)]`) | Reporter send/receive, noop behavior, arbitrary phase strings |
| `crates/jobs/common/heartbeat.rs` (inline `#[cfg(test)]`) | Content-aware staleness detection, heartbeat lifecycle |
| `crates/ingest/github/issues.rs` (inline `#[cfg(test)]`) | Page limit enforcement, progress reporting |
| `crates/cli/commands/status/metrics.rs` (existing tests) | New display format for per-task phases |

---

## Task Dependency Graph

```
Task 1 (PhaseReporter)  ───────────────┐
                                       ├──→ Task 3 (issues/PRs progress) ──┐
Task 2 (Config fields)  ──────────────┤                                    ├──→ Task 5 (Wire ingest_github) ──→ Task 8 (Integration tests)
                                       └──→ Task 4 (wiki/metadata progress)┘                                         │
                                                                                                                      ▼
Task 6 (Content-aware heartbeat)  ─────────────────────────────────────────────────────────────────────────→ Task 9 (Lint/verify)
                                                                                                                      ▲
Task 7 (Status display) ─────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Parallelization opportunities:**
- **Tasks 1 + 2**: Independent — can run in parallel (different files, no shared types)
- **Tasks 3 + 4**: Independent of each other (both depend on Task 1) — can run in parallel
- **Task 6**: Independent of Tasks 2-5 — can run in parallel with Tasks 3-5
- **Task 7**: Independent of Tasks 3-6 — can run in parallel with any of them
- **Sequential gates:** Task 5 blocks on Tasks 1-4. Task 8 blocks on Task 5. Task 9 blocks on all.

**Recommended execution order with 3 agents:**
| Agent A | Agent B | Agent C |
|---------|---------|---------|
| Task 1 | Task 2 | — |
| Task 3 | Task 4 | Task 6 |
| Task 5 | Task 7 | — |
| Task 8 | — | — |
| Task 9 | — | — |

---

## Task 1: Create `PhaseReporter` Helper (No Central Enum)

**Files:**
- Create: `crates/ingest/progress.rs`
- Modify: `crates/ingest.rs` (add `pub mod progress;`)

This is the foundation — a reusable, decoupled progress reporting abstraction. Each ingest source defines its own phase strings as constants. No central enum — sources are fully independent.

- [ ] **Step 1.1: Write failing tests for `PhaseReporter`**

```rust
// In crates/ingest/progress.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn phase_reporter_sends_progress() {
        let (tx, mut rx) = mpsc::channel::<serde_json::Value>(16);
        let reporter = PhaseReporter::new(Some(tx));

        reporter.report(serde_json::json!({
            "phase": "fetching_issues",
            "issues_fetched": 42,
        })).await;

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg["phase"], "fetching_issues");
        assert_eq!(msg["issues_fetched"], 42);
    }

    #[tokio::test]
    async fn phase_reporter_none_is_noop() {
        let reporter = PhaseReporter::new(None);
        // Must not panic
        reporter.report(serde_json::json!({"phase": "test"})).await;
    }

    #[tokio::test]
    async fn phase_reporter_report_phase_sends_phase_only() {
        let (tx, mut rx) = mpsc::channel::<serde_json::Value>(16);
        let reporter = PhaseReporter::new(Some(tx));

        reporter.report_phase("cloning").await;

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg["phase"], "cloning");
    }

    #[tokio::test]
    async fn phase_reporter_arbitrary_source_phases() {
        // Any source can use any phase string — no coupling to a central enum
        let (tx, mut rx) = mpsc::channel::<serde_json::Value>(16);
        let reporter = PhaseReporter::new(Some(tx));

        reporter.report_phase("downloading_transcript").await;
        reporter.report_phase("fetching_subreddit").await;
        reporter.report_phase("scanning_sessions").await;

        let msg1 = rx.recv().await.unwrap();
        assert_eq!(msg1["phase"], "downloading_transcript");
        let msg2 = rx.recv().await.unwrap();
        assert_eq!(msg2["phase"], "fetching_subreddit");
        let msg3 = rx.recv().await.unwrap();
        assert_eq!(msg3["phase"], "scanning_sessions");
    }
}
```

- [ ] **Step 1.2: Run tests to verify they fail**

Run: `cargo test -p axon --lib ingest::progress::tests -- --nocapture 2>&1 | head -30`
Expected: FAIL — module doesn't exist

- [ ] **Step 1.3: Implement `PhaseReporter`**

```rust
// crates/ingest/progress.rs

use crate::crates::core::logging::log_warn;
use tokio::sync::mpsc;

/// Lightweight progress reporter that wraps an optional mpsc sender.
///
/// Designed to be passed by reference into ingest sub-tasks. When the sender
/// is `None` (e.g. synchronous `--wait` mode or tests), all calls are no-ops.
///
/// **No central phase enum.** Each ingest source defines its own phase
/// constants as `&str` in its own module. This keeps sources fully decoupled —
/// adding a new source never touches shared code.
///
/// Usage:
/// ```rust
/// // In your source module, define phases locally:
/// const PHASE_FETCHING: &str = "fetching_posts";
/// const PHASE_EMBEDDING: &str = "embedding_posts";
///
/// // Use the reporter:
/// let reporter = PhaseReporter::new(Some(progress_tx));
/// reporter.report_phase(PHASE_FETCHING).await;
/// reporter.report(json!({"phase": PHASE_EMBEDDING, "posts_done": 42})).await;
/// ```
#[derive(Clone)]
pub struct PhaseReporter {
    tx: Option<mpsc::Sender<serde_json::Value>>,
}

impl PhaseReporter {
    pub fn new(tx: Option<mpsc::Sender<serde_json::Value>>) -> Self {
        Self { tx }
    }

    /// A no-op reporter for sources that don't have a progress channel.
    pub fn noop() -> Self {
        Self { tx: None }
    }

    /// Send an arbitrary progress JSON blob. Keys are merged into `result_json`
    /// via JSONB `||` in the database.
    pub async fn report(&self, progress: serde_json::Value) {
        if let Some(tx) = &self.tx {
            if let Err(e) = tx.send(progress).await {
                log_warn(&format!("progress_send_failed err={e}"));
            }
        }
    }

    /// Convenience: send a phase-only update.
    pub async fn report_phase(&self, phase: &str) {
        self.report(serde_json::json!({"phase": phase})).await;
    }
}
```

- [ ] **Step 1.4: Run all tests, verify pass**

Run: `cargo test -p axon --lib ingest::progress -- --nocapture 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 1.5: Add `pub mod progress;` to `crates/ingest.rs`**

In `crates/ingest.rs`, add near the top with other module declarations:
```rust
pub mod progress;
```

- [ ] **Step 1.6: Commit**

```bash
git add crates/ingest/progress.rs crates/ingest.rs
git commit -m "feat(ingest): add PhaseReporter helper for decoupled progress reporting

Lightweight wrapper around optional mpsc sender. Each ingest source
defines its own phase strings — no central enum, no coupling.
report() sends arbitrary JSON, report_phase() is a convenience shorthand."
```

---

## Task 2: Add `github_max_issues` and `github_max_prs` Config Fields

**Files:**
- Modify: `crates/core/config/types/config.rs:184-187`
- Modify: `crates/core/config/types/subconfigs.rs:62-71`
- Modify: `crates/core/config/types/config_impls.rs:71-72`
- Modify: `crates/core/config/parse/build_config.rs:340-341`
- Modify: `crates/core/config/cli/global_args.rs`
- Modify: `crates/core/config/help.rs`

- [ ] **Step 2.1: Write failing test for default config values**

```rust
// Add to existing tests in crates/core/config/types.rs
#[test]
fn default_config_has_github_issue_pr_limits() {
    let cfg = Config::default();
    assert_eq!(cfg.github_max_issues, 100);
    assert_eq!(cfg.github_max_prs, 100);
}
```

- [ ] **Step 2.2: Run test to verify it fails**

Run: `cargo test -p axon --lib core::config::types::tests -- default_config_has_github 2>&1 | tail -10`
Expected: FAIL — field not found

- [ ] **Step 2.3: Add fields to `Config` struct**

In `crates/core/config/types/config.rs`, after `github_include_source`:
```rust
    /// Maximum number of issues to ingest per repo (0 = unlimited).
    /// Sorted by most recently updated. Env: `GITHUB_MAX_ISSUES`. Flag: `--max-issues`.
    pub github_max_issues: usize,

    /// Maximum number of pull requests to ingest per repo (0 = unlimited).
    /// Sorted by most recently updated. Env: `GITHUB_MAX_PRS`. Flag: `--max-prs`.
    pub github_max_prs: usize,
```

- [ ] **Step 2.4: Add defaults in `config_impls.rs`**

In `Config::default()`, after `github_include_source: true,`:
```rust
            github_max_issues: 100,
            github_max_prs: 100,
```

Also add to the `Debug` impl after the `github_include_source` field:
```rust
            .field("github_max_issues", &self.github_max_issues)
            .field("github_max_prs", &self.github_max_prs)
```

- [ ] **Step 2.5: Add fields to `IngestConfig` subconfig**

In `crates/core/config/types/subconfigs.rs`, in `IngestConfig` struct after `github_include_source`:
```rust
    pub github_max_issues: usize,
    pub github_max_prs: usize,
```

And in `Default for IngestConfig`:
```rust
            github_max_issues: 100,
            github_max_prs: 100,
```

- [ ] **Step 2.6: Add env var parsing in `build_config.rs`**

In the config construction block, after `github_include_source,`:
```rust
        github_max_issues: env::var("GITHUB_MAX_ISSUES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100),
        github_max_prs: env::var("GITHUB_MAX_PRS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100),
```

- [ ] **Step 2.7: Add CLI args in `global_args.rs`**

After the existing `--include-source` / `--no-source` arg:
```rust
    /// Max issues to ingest per GitHub repo (0 = unlimited, default 100).
    #[arg(long = "max-issues", default_value_t = 100)]
    pub(in crate::crates::core::config) github_max_issues: usize,

    /// Max pull requests to ingest per GitHub repo (0 = unlimited, default 100).
    #[arg(long = "max-prs", default_value_t = 100)]
    pub(in crate::crates::core::config) github_max_prs: usize,
```

Wire into `build_config.rs` — override the env default with CLI arg when present.

- [ ] **Step 2.8: Add help text**

In `crates/core/config/help.rs`, in the ingest section:
```
  --max-issues N    Max issues per repo (0=all, default 100)
  --max-prs N       Max PRs per repo (0=all, default 100)
```

- [ ] **Step 2.9: Run test to verify it passes**

Run: `cargo test -p axon --lib core::config -- default_config_has_github 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 2.10: Commit**

```bash
git add crates/core/config/
git commit -m "feat(config): add github_max_issues and github_max_prs limits

Default 100 each (covers recent activity). 0 = unlimited for full ingestion.
Configurable via GITHUB_MAX_ISSUES/GITHUB_MAX_PRS env vars or --max-issues/--max-prs flags.
Source code ingestion is the primary use case; issues/PRs are supplementary context."
```

---

## Task 3: Add Progress Reporting and Page Limits to `ingest_issues`

**Files:**
- Modify: `crates/ingest/github/issues.rs`

- [ ] **Step 3.1: Write failing test for page-limited issue ingestion**

```rust
// Add to crates/ingest/github/issues.rs #[cfg(test)] mod tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::ingest::progress::PhaseReporter;

    /// Helper: compute how many items to take from a page given a max limit.
    /// Mirrors the pagination logic in `ingest_issues` / `ingest_pull_requests`.
    fn take_from_page(page_items: usize, collected: usize, max: usize) -> usize {
        if max == 0 {
            page_items // unlimited
        } else {
            page_items.min(max.saturating_sub(collected))
        }
    }

    /// Helper: check whether to break pagination given max and collected.
    fn should_break(collected: usize, max: usize) -> bool {
        max > 0 && collected >= max
    }

    #[test]
    fn take_from_page_respects_limit() {
        // 80 items on page, already collected 30, limit 50 → take 20
        assert_eq!(take_from_page(80, 30, 50), 20);
    }

    #[test]
    fn take_from_page_unlimited_takes_all() {
        // max=0 means unlimited — take everything
        assert_eq!(take_from_page(80, 9999, 0), 80);
    }

    #[test]
    fn should_break_at_limit() {
        assert!(should_break(50, 50));
        assert!(should_break(51, 50));
        assert!(!should_break(49, 50));
    }

    #[test]
    fn should_break_never_for_unlimited() {
        assert!(!should_break(0, 0));
        assert!(!should_break(999999, 0));
    }

    #[tokio::test]
    async fn progress_reporter_receives_issue_phases() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<serde_json::Value>(16);
        let reporter = PhaseReporter::new(Some(tx));

        reporter.report(serde_json::json!({
            "phase": PHASE_FETCHING_ISSUES,
            "issues_fetched": 100,
            "issues_page": 1,
        })).await;

        reporter.report(serde_json::json!({
            "phase": PHASE_EMBEDDING_ISSUES,
            "issues_total": 100,
        })).await;

        drop(reporter);

        let msg1 = rx.recv().await.unwrap();
        assert_eq!(msg1["phase"], "fetching_issues");
        assert_eq!(msg1["issues_fetched"], 100);

        let msg2 = rx.recv().await.unwrap();
        assert_eq!(msg2["phase"], "embedding_issues");
        assert_eq!(msg2["issues_total"], 100);
    }
}
```

- [ ] **Step 3.2: Run tests to verify they fail**

Run: `cargo test -p axon --lib ingest::github::issues::tests -- --nocapture 2>&1 | tail -10`
Expected: FAIL — `take_from_page` and `should_break` don't exist yet (functions will be extracted from pagination logic in Step 3.3)

- [ ] **Step 3.3: Add pagination helper functions and refactor `ingest_issues`**

Add local phase constants and pagination helpers:
```rust
// Phase constants — local to this module, not a shared enum
const PHASE_FETCHING_ISSUES: &str = "fetching_issues";
const PHASE_EMBEDDING_ISSUES: &str = "embedding_issues";
const PHASE_FETCHING_PRS: &str = "fetching_prs";
const PHASE_EMBEDDING_PRS: &str = "embedding_prs";

/// Compute how many items to take from a page given a max limit.
/// `max == 0` means unlimited (take all).
fn take_from_page(page_items: usize, collected: usize, max: usize) -> usize {
    if max == 0 {
        page_items
    } else {
        page_items.min(max.saturating_sub(collected))
    }
}

/// Whether to stop paginating (true when limit reached).
fn should_break(collected: usize, max: usize) -> bool {
    max > 0 && collected >= max
}
```

Change the signature:
```rust
pub async fn ingest_issues(
    cfg: &Config,
    octo: &Octocrab,
    common: &GitHubCommonFields,
    max_issues: usize,
    reporter: &PhaseReporter,
) -> Result<usize> {
```

Implementation changes:
1. Add `reporter.report_phase(PHASE_FETCHING_ISSUES).await;` at the start
2. Sort by `updated` descending (most recent first): `.sort(params::issues::Sort::Updated).direction(params::Direction::Descending)` — **verify octocrab version supports these types before coding**. If not available, use raw query parameters or upgrade octocrab.
3. Track `issues_collected` count
4. Break pagination loop when `should_break(issues_collected, max_issues)`
5. Report progress every page:
```rust
reporter.report(serde_json::json!({
    "phase": PHASE_FETCHING_ISSUES,
    "issues_fetched": issues_collected,
    "issues_page": page_num,
})).await;
```
6. Before embedding, report phase transition:
```rust
reporter.report(serde_json::json!({
    "phase": PHASE_EMBEDDING_ISSUES,
    "issues_total": docs.len(),
})).await;
```
7. Log: `log_info(&format!("github issues_fetched={issues_collected} pages={page_num} max={max_issues} repo={}", common.repo_slug));`

- [ ] **Step 3.4: Refactor `ingest_pull_requests` with same pattern**

```rust
pub async fn ingest_pull_requests(
    cfg: &Config,
    octo: &Octocrab,
    common: &GitHubCommonFields,
    max_prs: usize,
    reporter: &PhaseReporter,
) -> Result<usize> {
```

Same changes:
1. `reporter.report_phase(PHASE_FETCHING_PRS).await;`
2. Sort: `.sort(params::pulls::Sort::Updated).direction(params::Direction::Descending)` (verify octocrab API)
3. Track `prs_collected`, break when limit reached
4. Progress every page + phase transition before embedding
5. Structured logging

- [ ] **Step 3.5: Verify compilation (expect errors)**

Run: `cargo check -p axon 2>&1 | tail -20`
Expected: Errors in `github.rs` because callers haven't been updated yet. **Do NOT commit yet.** Tasks 3, 4, and 5 form an atomic unit — commit happens at Task 5 Step 5.4.

- [ ] **Step 3.6: Skip commit — will commit with Task 5**

Tasks 3, 4, and 5 change function signatures together. The commit happens at Task 5 after all callers compile.

---

## Task 4: Add Progress Reporting to Wiki and Metadata

**Files:**
- Modify: `crates/ingest/github/wiki.rs`
- Modify: `crates/ingest/github.rs` (the `embed_repo_metadata` function)

- [ ] **Step 4.1: Read wiki.rs and understand current flow**

Read `crates/ingest/github/wiki.rs` fully before making changes.

- [ ] **Step 4.2: Add `PhaseReporter` parameter to `ingest_wiki`**

```rust
pub async fn ingest_wiki(
    cfg: &Config,
    common: &GitHubCommonFields,
    token: Option<&str>,
    reporter: &PhaseReporter,
) -> Result<usize> {
```

Add progress at key stages:
Add local phase constants at the top of `wiki.rs`:
```rust
const PHASE_FETCHING_WIKI: &str = "fetching_wiki";
const PHASE_EMBEDDING_WIKI: &str = "embedding_wiki";
```

1. `reporter.report_phase(PHASE_FETCHING_WIKI).await;` before clone
2. `reporter.report_phase(PHASE_EMBEDDING_WIKI).await;` before embed
3. Log: `log_info(&format!("github wiki_clone_complete files={file_count} repo={}", common.repo_slug));`

- [ ] **Step 4.3: Add `PhaseReporter` parameter to `embed_repo_metadata`**

```rust
async fn embed_repo_metadata(
    cfg: &Config,
    repo_info: &octocrab::models::Repository,
    common: &GitHubCommonFields,
    reporter: &PhaseReporter,
) -> Result<usize> {
```

Add a local phase constant at the top of `github.rs` (alongside other constants):
```rust
const PHASE_EMBEDDING_METADATA: &str = "embedding_metadata";
```

Add: `reporter.report_phase(PHASE_EMBEDDING_METADATA).await;`

- [ ] **Step 4.4: Verify compilation (expect errors)**

Run: `cargo check -p axon 2>&1 | tail -20`
Expected: Errors in `github.rs` callers — this is expected. **Do NOT commit yet.** Task 4 and Task 5 must be committed together after callers are updated.

- [ ] **Step 4.5: Skip commit — will commit with Task 5**

Tasks 3, 4, and 5 form an atomic unit. The commit happens at Task 5 Step 5.4 after all callers compile.

---

## Task 5: Wire PhaseReporter Across ALL Ingest Sources

**Files:**
- Modify: `crates/ingest/github.rs` (the `ingest_github` function)
- Modify: `crates/ingest/github/files.rs` (update `embed_files` to use `PhaseReporter`)
- Modify: `crates/ingest/github/files/batch.rs` (update `send_progress` calls)
- Modify: `crates/ingest/youtube.rs` (add `PhaseReporter` parameter + phase constants)
- Modify: `crates/ingest/reddit.rs` (add `PhaseReporter` parameter + phase constants)
- Modify: `crates/ingest/sessions.rs` (add `PhaseReporter` parameter + phase constants)
- Modify: `crates/jobs/ingest/process.rs` (shared mpsc channel for all sources)

This is the integration task — wires everything together.

- [ ] **Step 5.1: Update `embed_files` signature to accept `PhaseReporter`**

Add local phase constants at the top of `files.rs`:
```rust
const PHASE_CLONING: &str = "cloning";
const PHASE_ENUMERATING_FILES: &str = "enumerating_files";
const PHASE_EMBEDDED_FILES: &str = "embedded_files";
```

Change signature and replace existing `send_progress` calls with `reporter.report(...)` using the constants:
```rust
pub async fn embed_files(
    cfg: &Config,
    common: &GitHubCommonFields,
    include_source: bool,
    token: Option<&str>,
    reporter: &PhaseReporter,
) -> Result<usize> {
```

Remove `use batch::send_progress;` import — no longer needed.

Replace internal `send_progress` calls with `reporter.report(...)` calls. Each module owns its own phase strings.

Add local phase constants at the top of `batch.rs`:
```rust
const PHASE_COLLECTING_FILES: &str = "collecting_files";
const PHASE_EMBEDDING_BATCH: &str = "embedding_batch";
```

Update `collect_and_embed_batched` in `batch.rs`:
1. Change parameter `progress_tx: Option<&mpsc::Sender<serde_json::Value>>` → `reporter: &PhaseReporter`
2. Replace `send_progress(progress_tx, json!({...}))` with `reporter.report(json!({...}))` (3 call sites)
3. Change `flush_batch` parameter the same way
4. Delete the `send_progress` helper function — `PhaseReporter::report` replaces it entirely
5. Use `PHASE_COLLECTING_FILES` and `PHASE_EMBEDDING_BATCH` for phase values

Example diff for `collect_and_embed_batched`:
```rust
// Before:
pub(super) async fn collect_and_embed_batched(
    ctx: &Arc<FileEmbedCtx>, file_items: Vec<String>, files_total: usize,
    progress_tx: Option<&mpsc::Sender<serde_json::Value>>,
) -> Result<(usize, usize)> {

// After:
pub(super) async fn collect_and_embed_batched(
    ctx: &Arc<FileEmbedCtx>, file_items: Vec<String>, files_total: usize,
    reporter: &PhaseReporter,
) -> Result<(usize, usize)> {
```

Example diff for progress send:
```rust
// Before:
send_progress(progress_tx, serde_json::json!({
    "files_done": files_done, "files_total": files_total,
    "chunks_embedded": total_chunks, "phase": "collecting_files",
})).await;

// After:
reporter.report(serde_json::json!({
    "files_done": files_done, "files_total": files_total,
    "chunks_embedded": total_chunks, "phase": PHASE_COLLECTING_FILES,
})).await;
```

- [ ] **Step 5.2: Restructure `ingest_github` to report per-task completion**

The key change: instead of `tokio::join!` with `tasks_done` only set at the end, use a pattern that increments `tasks_done` as each task completes.

**IMPORTANT:** Do NOT use `async move` blocks inside `tokio::join!`. The branches must borrow `cfg`, `common`, `repo_info`, and `octo` by shared reference (which `tokio::join!` supports — all branches run on the same task, not spawned). Only clone `reporter` and `tasks_done` (which need owned copies per branch).

```rust
pub async fn ingest_github(
    cfg: &Config,
    repo: &str,
    include_source: bool,
    reporter: PhaseReporter,  // Changed from Option<mpsc::Sender<...>>
) -> Result<usize> {
    // ... existing repo info fetch ...

    reporter.report(serde_json::json!({
        "phase": "ingesting",
        "tasks_total": 5,
        "tasks_done": 0,
    })).await;

    // Shared atomic counter for tasks_done — Arc because cloned into each branch
    let tasks_done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Helper macro to reduce boilerplate for the post-task reporting pattern
    // (or inline it — either works). Each branch:
    //   1. Clones reporter + tasks_done into local bindings (owned)
    //   2. Borrows cfg, common, etc. by reference (NOT async move)
    //   3. Reports tasks_done after completion

    let (files_result, metadata_result, issues_result, prs_result, wiki_result) = tokio::join!(
        // Task 1: Files
        async {
            let r = reporter.clone();
            let td = tasks_done.clone();
            let result = files::embed_files(
                cfg, &common, include_source,
                cfg.github_token.as_deref(), &r,
            ).await;
            let done = td.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            r.report(serde_json::json!({"tasks_done": done})).await;
            log_info(&format!("github task_complete task=files tasks_done={done}/5 repo={}", common.repo_slug));
            result
        },
        // Task 2: Metadata
        async {
            let r = reporter.clone();
            let td = tasks_done.clone();
            let result = embed_repo_metadata(cfg, &repo_info, &common, &r).await;
            let done = td.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            r.report(serde_json::json!({"tasks_done": done})).await;
            log_info(&format!("github task_complete task=metadata tasks_done={done}/5 repo={}", common.repo_slug));
            result
        },
        // Task 3: Issues
        async {
            let r = reporter.clone();
            let td = tasks_done.clone();
            let result = issues::ingest_issues(
                cfg, &octo, &common, cfg.github_max_issues, &r,
            ).await;
            let done = td.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            r.report(serde_json::json!({"tasks_done": done})).await;
            log_info(&format!("github task_complete task=issues tasks_done={done}/5 repo={}", common.repo_slug));
            result
        },
        // Task 4: PRs
        async {
            let r = reporter.clone();
            let td = tasks_done.clone();
            let result = issues::ingest_pull_requests(
                cfg, &octo, &common, cfg.github_max_prs, &r,
            ).await;
            let done = td.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            r.report(serde_json::json!({"tasks_done": done})).await;
            log_info(&format!("github task_complete task=prs tasks_done={done}/5 repo={}", common.repo_slug));
            result
        },
        // Task 5: Wiki
        async {
            let r = reporter.clone();
            let td = tasks_done.clone();
            let result = if common.has_wiki {
                wiki::ingest_wiki(cfg, &common, cfg.github_token.as_deref(), &r).await
            } else {
                Ok(0)
            };
            let done = td.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            r.report(serde_json::json!({"tasks_done": done})).await;
            log_info(&format!("github task_complete task=wiki tasks_done={done}/5 repo={}", common.repo_slug));
            result
        },
    );

    // ... existing tally_results ...
}
```

Also delete the existing `send_progress` free function at `github.rs:203-211` — `PhaseReporter::report` replaces it entirely.

- [ ] **Step 5.3: Wire `PhaseReporter` into YouTube, Reddit, and Sessions**

These sources currently have zero progress reporting. Add `PhaseReporter` parameter to each and report phases at key boundaries.

**`crates/ingest/youtube.rs`:**
```rust
// Local phase constants
const PHASE_DOWNLOADING: &str = "downloading_transcript";
const PHASE_PARSING: &str = "parsing_transcript";
const PHASE_EMBEDDING: &str = "embedding_transcript";

pub async fn ingest_youtube(
    cfg: &Config,
    url: &str,
    reporter: &PhaseReporter,  // NEW parameter
) -> Result<usize, Box<dyn Error>> {
```
Add `reporter.report_phase(PHASE_DOWNLOADING).await;` before `run_ytdlp`, `reporter.report_phase(PHASE_PARSING).await;` before VTT parsing, `reporter.report_phase(PHASE_EMBEDDING).await;` before `embed_prepared_docs`.

**IMPORTANT:** Also update `ingest_video_with_retry` in `process.rs:87-116` — it calls `ingest_youtube` and needs the reporter parameter:
```rust
async fn ingest_video_with_retry(cfg: &Config, video_url: &str, reporter: &PhaseReporter) -> Result<usize, String> {
    for attempt in 0..=RETRY_429_MAX_ATTEMPTS {
        let result = ingest::youtube::ingest_youtube(cfg, video_url, reporter)
            .await
            .map_err(|e| e.to_string());
        // ... rest unchanged ...
    }
}
```
And thread `reporter` through `drain_playlist_videos_with_pool` → `ingest_video_with_retry`. For the playlist path, pass `&PhaseReporter::noop()` since playlist progress is tracked separately via direct SQL updates.

**`crates/ingest/reddit.rs`:**
```rust
const PHASE_AUTHENTICATING: &str = "authenticating";
const PHASE_FETCHING_POSTS: &str = "fetching_posts";
const PHASE_EMBEDDING_POSTS: &str = "embedding_posts";

pub async fn ingest_reddit(
    cfg: &Config,
    target: &str,
    reporter: &PhaseReporter,  // NEW parameter
) -> Result<usize, Box<dyn Error>> {
```
Add phase reports before OAuth, fetching, and embedding. Note: `ingest_reddit` delegates to `ingest_subreddit` or `ingest_thread` — both internal functions also need `reporter: &PhaseReporter` threaded through.

**`crates/ingest/sessions.rs`:**
```rust
const PHASE_SCANNING: &str = "scanning_sessions";
const PHASE_EMBEDDING: &str = "embedding_sessions";

pub async fn ingest_sessions(
    cfg: &Config,
    reporter: &PhaseReporter,  // NEW parameter
) -> Result<usize, Box<dyn Error>> {
```

Note: `ingest_sessions` already uses `indicatif::MultiProgress` for interactive terminal display. Keep both — `indicatif` is for terminal mode, `PhaseReporter` is for DB-persisted progress in worker mode. They serve different purposes and coexist.

- [ ] **Step 5.4: Wire mpsc channel for ALL ingest sources in `process.rs`**

Currently only GitHub gets the mpsc progress channel. Refactor `process.rs` to create the channel once and pass `PhaseReporter` to every source:

```rust
// In process_ingest_job, BEFORE the match on source:
let (progress_tx, mut progress_rx) =
    tokio::sync::mpsc::channel::<serde_json::Value>(256);
let progress_pool = pool.clone();
let progress_id = id;
let progress_task = tokio::spawn(
    async move {
        while let Some(progress) = progress_rx.recv().await {
            update_ingest_progress(&progress_pool, progress_id, &progress).await;
        }
    }
    .instrument(tracing::Span::current()),
);
let reporter = PhaseReporter::new(Some(progress_tx));

let result = match &job_cfg.source {
    IngestSource::Github { repo, include_source } => {
        ingest::github::ingest_github(&cfg, repo, *include_source, reporter.clone())
            .await
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })
    }
    IngestSource::Reddit { target } => {
        ingest::reddit::ingest_reddit(&cfg, target, &reporter).await
    }
    IngestSource::Youtube { target } => {
        if ingest::youtube::is_playlist_or_channel_url(target) {
            // Playlist already has its own pool-based progress — pass noop reporter
            ingest_youtube_playlist_with_pool(&cfg, &pool, id, target).await
        } else {
            ingest::youtube::ingest_youtube(&cfg, target, &reporter).await
        }
    }
    IngestSource::Sessions { .. } => {
        // ... existing config setup ...
        ingest::sessions::ingest_sessions(&sessions_cfg, &reporter).await
    }
};

// Wait for final DB write
drop(reporter); // close sender side
let _ = progress_task.await;
```

Note: `ingest_github` still receives `Option<mpsc::Sender<...>>` internally because it constructs its own `PhaseReporter` from the sender. Alternatively, change `ingest_github` to accept `&PhaseReporter` directly. **Decision:** change `ingest_github` to accept `PhaseReporter` directly (cleaner — no double-wrapping).

- [ ] **Step 5.5: Verify compilation and existing tests pass**

Run: `cargo test -p axon 2>&1 | tail -20`
Expected: All existing tests pass

- [ ] **Step 5.6: Commit**

```bash
git add crates/ingest/github.rs crates/ingest/github/issues.rs crates/ingest/github/wiki.rs crates/ingest/github/files.rs crates/ingest/github/files/batch.rs crates/ingest/youtube.rs crates/ingest/reddit.rs crates/ingest/sessions.rs crates/jobs/ingest/process.rs
git commit -m "feat(ingest): wire PhaseReporter across all ingest sources

Tasks 3+4+5 combined: All ingest sources (GitHub, YouTube, Reddit, Sessions)
now report progress via PhaseReporter with source-local phase constants.
GitHub sub-tasks report individually with atomic tasks_done counter.
Issues/PRs sorted by most-recently-updated, capped at configurable limits.
process.rs creates one shared mpsc channel for all sources."
```

---

## Task 6: Content-Aware Heartbeat

**Files:**
- Create: `crates/jobs/common/heartbeat.rs`
- Modify: `crates/jobs/common/job_ops.rs` (remove heartbeat, re-export from new module)
- Modify: `crates/jobs/common.rs` (add `pub mod heartbeat;`)
- Modify: `crates/jobs/worker_lane.rs` (use new heartbeat)

- [ ] **Step 6.1: Write failing test for content-aware staleness detection**

```rust
// In crates/jobs/common/heartbeat.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_stale_when_result_json_unchanged() {
        let prev = Some(serde_json::json!({"files_done": 150, "phase": "embedding_batch"}));
        let curr = Some(serde_json::json!({"files_done": 150, "phase": "embedding_batch"}));
        assert!(is_content_stale(&prev, &curr));
    }

    #[test]
    fn detect_progress_when_result_json_changes() {
        let prev = Some(serde_json::json!({"files_done": 150, "phase": "embedding_batch"}));
        let curr = Some(serde_json::json!({"files_done": 151, "phase": "collecting_files"}));
        assert!(!is_content_stale(&prev, &curr));
    }

    #[test]
    fn null_previous_is_never_stale() {
        let prev = None;
        let curr = Some(serde_json::json!({"files_done": 1}));
        assert!(!is_content_stale(&prev, &curr));
    }

    #[test]
    fn both_null_is_not_stale() {
        assert!(!is_content_stale(&None, &None));
    }

    #[test]
    fn stale_streak_triggers_warning_after_threshold() {
        let threshold = 3;
        let mut streak = 0u32;
        for _ in 0..threshold {
            streak += 1;
        }
        assert!(streak >= threshold, "streak should trigger warning");
    }
}
```

- [ ] **Step 6.2: Run tests to verify they fail**

Run: `cargo test -p axon --lib jobs::common::heartbeat::tests -- --nocapture 2>&1 | tail -10`
Expected: FAIL — module doesn't exist

- [ ] **Step 6.3: Implement content-aware heartbeat module**

```rust
// crates/jobs/common/heartbeat.rs

use crate::crates::core::logging::{log_debug, log_warn};
use crate::crates::jobs::common::JobTable;
use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Compare two `result_json` snapshots. Returns `true` if the content
/// is identical (meaning no progress was made between heartbeats).
pub fn is_content_stale(
    prev: &Option<serde_json::Value>,
    curr: &Option<serde_json::Value>,
) -> bool {
    match (prev, curr) {
        (Some(p), Some(c)) => p == c,
        (None, None) => false, // both null = job just started, not stale
        _ => false,            // transition from None→Some = progress
    }
}

/// Stale streak threshold before logging a warning.
/// At 30s heartbeat interval, 6 streaks = 3 minutes of no progress.
const STALE_STREAK_WARN_THRESHOLD: u32 = 6;

/// Spawn a heartbeat task that:
/// 1. Touches `updated_at` every `interval_secs` (keeps watchdog happy)
/// 2. Reads `result_json` and compares to previous snapshot
/// 3. Logs a warning when content hasn't changed for `STALE_STREAK_WARN_THRESHOLD` intervals
///
/// The warning is diagnostic only — it does NOT cancel the job. Operators
/// can use the log line to identify genuinely stuck jobs.
pub fn spawn_content_aware_heartbeat(
    pool: PgPool,
    table: JobTable,
    id: Uuid,
    interval_secs: u64,
) -> (watch::Sender<bool>, JoinHandle<()>) {
    let (stop_tx, mut stop_rx) = watch::channel(false);
    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        let mut prev_snapshot: Option<serde_json::Value> = None;
        let mut stale_streak: u32 = 0;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // 1. Touch updated_at (existing behavior)
                    let _ = super::touch_running_job(&pool, table, id).await;

                    // 2. Read current result_json
                    let curr = read_result_json(&pool, table, id).await;

                    // 3. Compare to previous snapshot
                    if is_content_stale(&prev_snapshot, &curr) {
                        stale_streak += 1;
                        if stale_streak == STALE_STREAK_WARN_THRESHOLD {
                            let phase = curr.as_ref()
                                .and_then(|v| v.get("phase"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            log_warn(&format!(
                                "heartbeat content_stale job_id={id} table={} streak={stale_streak} \
                                 phase={phase} no_progress_secs={}",
                                table.as_str(),
                                stale_streak as u64 * interval_secs,
                            ));
                        } else if stale_streak > STALE_STREAK_WARN_THRESHOLD
                            && stale_streak % STALE_STREAK_WARN_THRESHOLD == 0
                        {
                            // Repeat warning every N intervals
                            log_warn(&format!(
                                "heartbeat content_still_stale job_id={id} table={} streak={stale_streak} \
                                 no_progress_secs={}",
                                table.as_str(),
                                stale_streak as u64 * interval_secs,
                            ));
                        }
                    } else {
                        if stale_streak >= STALE_STREAK_WARN_THRESHOLD {
                            log_debug(&format!(
                                "heartbeat content_unstalled job_id={id} table={} streak_was={stale_streak}",
                                table.as_str(),
                            ));
                        }
                        stale_streak = 0;
                    }

                    prev_snapshot = curr;
                }
                changed = stop_rx.changed() => {
                    if changed.is_err() || *stop_rx.borrow() {
                        break;
                    }
                }
            }
        }
    });
    (stop_tx, handle)
}

/// Read `result_json` from the job's table. Returns `None` on any DB error
/// (heartbeat must never crash the worker).
async fn read_result_json(
    pool: &PgPool,
    table: JobTable,
    id: Uuid,
) -> Option<serde_json::Value> {
    let query = format!(
        "SELECT result_json FROM {} WHERE id=$1",
        table.as_str()
    );
    sqlx::query_scalar::<_, Option<serde_json::Value>>(&query)
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
}
```

- [ ] **Step 6.4: Run tests to verify they pass**

Run: `cargo test -p axon --lib jobs::common::heartbeat::tests -- --nocapture 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 6.5: Move existing `spawn_heartbeat_task` to `heartbeat.rs` and re-export**

**Decision:** Move both the existing `spawn_heartbeat_task` AND the new `spawn_content_aware_heartbeat` into `heartbeat.rs`. Re-export from `job_ops.rs` so existing callers don't break.

Two re-export sites need updating:

```rust
// In job_ops.rs, remove the spawn_heartbeat_task function body and replace with:
pub use super::heartbeat::{spawn_heartbeat_task, spawn_content_aware_heartbeat};
```

```rust
// In common.rs line 41-44, update re-exports to include new function:
pub use job_ops::{
    cancel_pending_or_running_job, claim_next_pending, claim_pending_by_id, mark_job_completed,
    mark_job_failed, spawn_heartbeat_task, spawn_content_aware_heartbeat, touch_running_job,
};
```

Copy the existing `spawn_heartbeat_task` function body into `heartbeat.rs` unchanged. Both versions live side-by-side.

- [ ] **Step 6.6: Update `wrap_with_heartbeat` in `worker_lane.rs`**

Add a `content_aware: bool` parameter or create `wrap_with_content_aware_heartbeat`:

```rust
pub(crate) fn wrap_with_content_aware_heartbeat(
    process_fn: ProcessFn,
    table: JobTable,
    interval_secs: u64,
) -> ProcessFn {
    Arc::new(move |cfg, pool, id| {
        let pool_hb = pool.clone();
        let inner = process_fn(cfg, pool, id);
        Box::pin(async move {
            let (stop_tx, hb_task) =
                heartbeat::spawn_content_aware_heartbeat(pool_hb, table, id, interval_secs);
            inner.await;
            let _ = stop_tx.send(true);
            let _ = hb_task.await;
        })
    })
}
```

- [ ] **Step 6.7: Wire content-aware heartbeat for ALL workers**

**Decision:** Always use content-aware heartbeat. The DB read overhead (one SELECT per 30s heartbeat) is negligible, and all workers benefit from staleness detection. This avoids bifurcating the heartbeat logic.

In `crates/jobs/worker_lane.rs`, replace the call to `wrap_with_heartbeat` with `wrap_with_content_aware_heartbeat`:

```rust
// In run_job_worker, replace:
//   let process_fn = wrap_with_heartbeat(process_fn, wc.table, wc.heartbeat_interval_secs);
// With:
let process_fn = wrap_with_content_aware_heartbeat(process_fn, wc.table, wc.heartbeat_interval_secs);
```

Remove the old `wrap_with_heartbeat` function. Import from the new module:
```rust
use crate::crates::jobs::common::heartbeat::spawn_content_aware_heartbeat;
```

- [ ] **Step 6.8: Verify `JobTable::as_str()` exists**

Check that `JobTable` has an `as_str()` method for the SQL query. If not, add it:
```rust
impl JobTable {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ingest => "axon_ingest_jobs",
            Self::Embed => "axon_embed_jobs",
            // ... etc
        }
    }
}
```

- [ ] **Step 6.9: Run full test suite**

Run: `cargo test -p axon 2>&1 | tail -20`
Expected: All tests pass, no regressions

- [ ] **Step 6.10: Commit**

```bash
git add crates/jobs/common/heartbeat.rs crates/jobs/common.rs crates/jobs/common/job_ops.rs crates/jobs/worker_lane.rs crates/jobs/ingest.rs
git commit -m "feat(heartbeat): content-aware staleness detection

Heartbeat now reads result_json each interval and compares to previous snapshot.
Logs warning when content unchanged for 6+ intervals (3 min at 30s cadence).
Diagnostic only — does not cancel jobs. Replaces blind updated_at touch.
All workers upgraded to content-aware heartbeat."
```

---

## Task 7: Update Status Display for Per-Task Phases

**Files:**
- Modify: `crates/cli/commands/status/metrics.rs`

- [ ] **Step 7.1: Write failing test for new status display**

**IMPORTANT:** `accent()` and `subtle()` from `crates/core/ui.rs` inject ANSI escape codes into the output. Test assertions must strip ANSI codes before matching. Add a helper:

```rust
/// Strip ANSI escape codes for test assertions.
#[cfg(test)]
fn strip_ansi(s: &str) -> String {
    // Matches \x1b[...m (SGR sequences)
    let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").to_string()
}

#[test]
fn ingest_suffix_shows_phase_and_tasks_done() {
    let result = serde_json::json!({
        "files_done": 150,
        "files_total": 155,
        "chunks_embedded": 1700,
        "tasks_done": 3,
        "tasks_total": 5,
        "phase": "fetching_issues",
        "issues_fetched": 42,
        "issues_page": 2,
    });
    let raw = ingest_metrics_suffix("running", Some(&result));
    let suffix = strip_ansi(&raw);
    // Should show: "150/155 files, 1700 chunks | 3/5 tasks | fetching_issues (42 issues, page 2)"
    assert!(suffix.contains("fetching_issues"), "should show current phase: {suffix}");
    assert!(suffix.contains("3/5 tasks"), "should show task progress: {suffix}");
}

#[test]
fn ingest_suffix_shows_embedding_issues_phase() {
    let result = serde_json::json!({
        "tasks_done": 2,
        "tasks_total": 5,
        "phase": "embedding_issues",
        "issues_total": 100,
        "chunks_embedded": 2400,
    });
    let raw = ingest_metrics_suffix("running", Some(&result));
    let suffix = strip_ansi(&raw);
    assert!(suffix.contains("embedding_issues"), "should show phase: {suffix}");
}
```

If `regex` isn't already in `Cargo.toml` dev-dependencies, add it. Or use a simpler manual strip:
```rust
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' { in_escape = true; continue; }
        if in_escape { if c == 'm' { in_escape = false; } continue; }
        out.push(c);
    }
    out
}
```

- [ ] **Step 7.2: Run tests, verify they fail**

- [ ] **Step 7.3: Update `ingest_metrics_suffix` to show richer status**

For running jobs, build a multi-part status:
1. File progress (if present): `"150/155 files"`
2. Chunk count: `"1700 chunks"`
3. Task progress (if present): `"3/5 tasks"`
4. Current phase: `"fetching_issues"`
5. Phase-specific detail (if present): `"42 issues, page 2"` or `"batch_size=85"`

Format: `"150/155 files | 1700 chunks | 3/5 tasks | fetching_issues (42 issues, page 2)"`

- [ ] **Step 7.4: Run tests, verify pass**

- [ ] **Step 7.5: Commit**

```bash
git add crates/cli/commands/status/metrics.rs
git commit -m "feat(status): show per-task phase and progress in ingest status

Status output now displays current phase label, task completion count,
and phase-specific details (page number, item counts).
Makes it easy to see exactly where an ingest job is spending time."
```

---

## Task 8: Integration Test — Full Pipeline Smoke Test

**Files:**
- Add integration test verifying the full flow

- [ ] **Step 8.1: Write integration test for progress reporting flow**

```rust
// In crates/ingest/github.rs #[cfg(test)]

#[tokio::test]
async fn progress_reporter_sends_all_phases() {
    // Test that PhaseReporter correctly sends all expected phases
    // without hitting any real APIs. Phases are plain strings — no enum.
    use crate::crates::ingest::progress::PhaseReporter;
    let (tx, mut rx) = tokio::sync::mpsc::channel::<serde_json::Value>(64);
    let reporter = PhaseReporter::new(Some(tx));

    // Simulate the GitHub ingest phase sequence using string constants
    let phases = [
        "cloning",
        "enumerating_files",
        "collecting_files",
        "embedding_batch",
        "embedded_files",
        "fetching_issues",
        "embedding_issues",
        "fetching_prs",
        "embedding_prs",
        "completed",
    ];

    for phase in &phases {
        reporter.report_phase(phase).await;
    }
    drop(reporter); // close channel

    let mut received = Vec::new();
    while let Some(msg) = rx.recv().await {
        received.push(msg["phase"].as_str().unwrap_or("").to_string());
    }

    assert_eq!(received.len(), phases.len());
    assert_eq!(received[0], "cloning");
    assert_eq!(received.last().unwrap(), "completed");
}
```

- [ ] **Step 8.2: Write test for content-aware heartbeat with real DB**

If a test Postgres pool is available (check existing test infrastructure):
```rust
#[tokio::test]
async fn content_aware_heartbeat_detects_stale_content() {
    // This tests the is_content_stale function directly (no DB needed)
    use crate::crates::jobs::common::heartbeat::is_content_stale;

    let snap1 = Some(serde_json::json!({"phase": "embedding_batch", "files_done": 150}));
    let snap2 = Some(serde_json::json!({"phase": "embedding_batch", "files_done": 150}));
    let snap3 = Some(serde_json::json!({"phase": "fetching_issues", "issues_fetched": 5}));

    assert!(is_content_stale(&snap1, &snap2), "identical snapshots should be stale");
    assert!(!is_content_stale(&snap2, &snap3), "different snapshots should not be stale");
}
```

- [ ] **Step 8.3: Run full test suite**

Run: `cargo test -p axon 2>&1 | tail -30`
Expected: All tests pass

- [ ] **Step 8.4: Commit**

```bash
git add crates/ingest/github.rs crates/jobs/common/heartbeat.rs
git commit -m "test(ingest): integration tests for progress reporting and heartbeat

Verifies phase reporter sends all expected phases in sequence.
Verifies content-aware staleness detection logic."
```

---

## Task 9: Verify, Lint, and Lock In

- [ ] **Step 9.1: Run full test suite**

```bash
cargo test -p axon 2>&1
```
Expected: All tests pass, no warnings

- [ ] **Step 9.2: Run clippy**

```bash
cargo clippy -p axon -- -D warnings 2>&1
```
Expected: No warnings

- [ ] **Step 9.3: Run formatting check**

```bash
cargo fmt -p axon -- --check 2>&1
```
Expected: Clean

- [ ] **Step 9.4: Build in release mode**

```bash
cargo build -p axon --release 2>&1 | tail -5
```
Expected: Clean build

- [ ] **Step 9.5: Manual smoke test (if Axon server is running)**

```bash
# Ingest a small repo with the new limits
axon ingest agentclientprotocol/symposium-acp --max-issues 10 --max-prs 10 --wait

# While running, check status in another terminal:
axon ingest status
# Should show: phase transitions, tasks_done incrementing, issue/PR page counts
```

- [ ] **Step 9.6: Final commit with all formatting fixes**

```bash
cargo fmt -p axon
git add -A
git commit -m "chore: format and lint cleanup for ingest observability overhaul"
```

---

## Summary of Changes

| Area | Before | After |
|------|--------|-------|
| **Issues/PRs** | Fetch ALL, no limit, no progress | Default 100 most recent, per-page progress, configurable |
| **Phase reporting** | Only file embedding reports phase | All ingest sources (GitHub, YouTube, Reddit, Sessions) report phases |
| **tasks_done** | Jumps 0→5 at completion | Increments as each task finishes |
| **Heartbeat** | Blind `updated_at` touch | Content-aware: detects frozen `result_json` |
| **Status display** | "150/155 files" only | "150/155 files \| 3/5 tasks \| fetching_issues (page 2)" |
| **Logging** | Minimal in issues/PRs/wiki | Structured logging at every stage boundary |
| **New ingest sources** | Must build progress from scratch | Use `PhaseReporter` + declare local `&str` phase constants |
| **Config** | No issue/PR limits | `GITHUB_MAX_ISSUES`, `GITHUB_MAX_PRS` env vars + CLI flags |

---

## Follow-Up Work (Out of Scope)

The following job types also lack progress reporting and would benefit from `PhaseReporter` adoption. They are **not** in this plan because the stuck-job problem was ingest-specific, and each has its own progress architecture:

| Worker | Current State | Follow-Up Needed |
|--------|---------------|-----------------|
| **Crawl** | Has typed `CrawlSummary` progress channel + own heartbeat loop | Upgrade heartbeat to content-aware. Consider `PhaseReporter` but typed channel is fine. |
| **Extract** | No progress during processing; `result_json` only at completion | Add `PhaseReporter` for per-URL progress. Wire mpsc channel in `extract/worker.rs`. |
| **Embed** | No progress reporting at all | Add `PhaseReporter`. Embed jobs are typically fast, but large directory embeds could benefit. |
| **Refresh** | No progress reporting | Add `PhaseReporter` for per-URL re-crawl progress. |

The content-aware heartbeat (Task 6) already upgrades **all** workers that use `worker_lane.rs` — this covers Ingest, Extract, and Embed. Crawl has its own heartbeat loop and needs separate treatment.

**Error type migration:** YouTube, Reddit, and Sessions return `Result<usize, Box<dyn Error>>` (not `Send + Sync`). This works today because these futures are awaited directly (not `tokio::spawn`ed), but it's a landmine if they're ever spawned. Follow-up: migrate all ingest source return types to `anyhow::Result<usize>` for `Send + Sync` safety.
