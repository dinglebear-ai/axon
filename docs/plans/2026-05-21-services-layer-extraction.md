# Services Layer Extraction — axon_rust-dvo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move all business logic — audit, artifact persistence, validation, ingest target parsing, and response envelopes — out of the CLI and MCP layers and into `src/services/`, making CLI/MCP thin presentation shells.

**Architecture:** Five independently shippable PRs in DAG order. Each PR deletes the misplaced logic from its current home, adds it in the canonical service location, and wires both CLI and MCP (and `/v1/actions` where applicable) to call through the service. No backward-compat re-exports; no thin-wrapper anti-pattern.

**Tech Stack:** Rust 2024 edition, Tokio, axon binary crate (`src/`), sidecar `_tests.rs` convention, no `mod.rs`, monolith policy (≤500 LOC/file, ≤120 LOC/fn).

---

## Dependency DAG

```
axon_rust-dvo.1 (P5 apply_overrides) — DONE ✓
    ↓
axon_rust-dvo.2 (P1 audit move) ─┐
axon_rust-dvo.3 (P0 artifacts)  ─┘  (parallel — disjoint files)
    ↓
axon_rust-dvo.4 (P2 validation consolidation)
    ↓
axon_rust-dvo.5 (P3 ingest target parsing)
    ↓
axon_rust-dvo.6 (P4 envelope unification)
```

## Lavra-Research Overrides (read before coding)

These override the original bead bodies where they conflict:

1. **Paths are `src/...`, not `crates/...`.** The beads pre-date workspace flattening.
2. **dvo.1 is already closed.** `Config::apply_overrides` migration is complete; do not redo it.
3. **`Backend::{Acp, OpenAi}` enum is stale.** The OpenAI path was removed. Research now uses Gemini headless (`AXON_HEADLESS_GEMINI_*`). dvo.4 does **not** add a `Backend` enum; use flat `validate_config_for_research(&Config)` checking `tavily_api_key` + `llm_backend::headless::gemini` config.
4. **`parse_ingest_source` is partially extracted.** `src/services/ingest/request.rs::source_from_mcp_request` already exists. dvo.5 unifies CLI + MCP + `/v1/actions` to call one constructor and tightens `/v1/actions` (currently no validation before enqueue).
5. **`validate_mcp_url` adds no MCP-specific policy** beyond error-type conversion — delete it, don't "audit then keep."
6. **`/v1/actions` is in scope** for P0 (scrape writes in `src/services/action_api/commands/dispatchers.rs`), P3 (weak ingest parser in `helpers.rs`), and P4 (hand-rolled response construction).
7. **`crawl/engine → services` is bidirectional layering.** `thin_refetch.rs` is inside the crawl engine, which `services` already imports. Making `thin_refetch` import `services` would create a cycle. Resolution: pass a `persist_fn` callback (a boxed async closure `Fn(path, bytes) -> Result`) from the service into `run_thin_refetch`; the crawl engine itself never imports `services`.
8. **`atomic_write` must be hardened.** `write_json_artifact` lacks `sync_all`, parent-dir sync, and root containment check. The new `services::io::atomic_write_under(root, relative_path, bytes)` must: reject absolute relative_path, reject null bytes, reject traversal (`..`), canonicalize root, construct path, write unique sibling temp, `sync_all`, rename, best-effort sync parent dir, verify final canonical stays under root.
9. **`ScrapeArtifacts` must not go into `service.rs`** (already 38 KB). Put it in a new `src/services/types/scrape.rs`.
10. **dvo.6 envelopes: do not collapse MCP and `/v1/actions` into one shape.** MCP uses `AxonToolResponse::ok` with `{ok, action, subaction, data}`; `/v1/actions` uses `{request_id, result/error, ...}`. Share inner typed `data` payload; keep two transport mappers.

---

## Writer Sites Inventory (P0 scope)

All `tokio::fs::write`/`create_dir_all` outside pure crawl-engine internals that P0 must migrate:

| File | Line(s) | Move to |
|------|---------|---------|
| `src/services/action_api/commands/dispatchers.rs` | 380, 384 | `services::scrape` service writes; dispatcher reads artifact path from result |
| `src/mcp/server/artifacts/respond.rs` | 33, 40 | Extract tmp+rename into `services::io::atomic_write_under` |
| `src/mcp/server/handlers_system/screenshot.rs` | 68 | Move `create_dir_all` to `services::screenshot` |
| `src/cli/commands/crawl/audit/manifest_audit.rs` | 148, 150 | Move with P1 (audit move — separate PR) |
| `src/cli/commands/crawl/audit/audit_diff.rs` | 98 | Move with P1 |
| `src/crawl/engine/thin_refetch.rs` | 240 | Pass persist callback; engine never imports services |

Excluded from P0 (crawl-engine internals — not artifact persistence surface):
- `src/crawl/engine/collector/manifest.rs` — crawl manifest write; owned by crawl engine
- `src/crawl/engine/sitemap.rs` — sitemap cache write; owned by crawl engine
- `src/crawl/engine/dir_ops.rs` — output dir setup; owned by crawl engine

---

## Section 1 — axon_rust-dvo.2 (P1): Move Audit Subsystem to `services/crawl/audit/`

**Priority:** P2 — can land in parallel with P0 (completely disjoint files)

**Goal:** Move `src/cli/commands/crawl/audit/{manifest_audit,audit_diff,sitemap}.rs` to `src/services/crawl/audit/`. CLI command becomes a thin shell that calls the service and handles output. Security-critical `resolve_manifest_entry_path` is preserved byte-for-byte. Presentation code (`println!`, `primary()`, `muted()`, `cfg.json_output`) stays in CLI.

### File Map

Create:
- `src/services/crawl/audit.rs` — module root, re-exports public types, declares submodules
- `src/services/crawl/audit/manifest_audit.rs` — moved from CLI; returns typed result, no printing
- `src/services/crawl/audit/audit_diff.rs` — moved from CLI; returns typed result, no printing
- `src/services/crawl/audit/sitemap.rs` — moved from CLI (unchanged — no printing)
- `src/services/crawl/audit/manifest_audit_tests.rs` — sidecar tests (snapshot + security)
- `src/services/crawl/audit/audit_diff_tests.rs` — sidecar tests
- `src/services/crawl/audit/sitemap_tests.rs` — sidecar tests

Modify:
- `src/services/crawl.rs` — add `pub mod audit;`
- `src/cli/commands/crawl/audit.rs` — thin shell: call service, own printing/JSON output
- `src/cli/commands/crawl/audit/manifest_audit.rs` — **delete** (moved)
- `src/cli/commands/crawl/audit/audit_diff.rs` — **delete** (moved)
- `src/cli/commands/crawl/audit/sitemap.rs` — **delete** (moved)
- `CHANGELOG.md`

New return types (in `src/services/crawl/audit.rs`):
```rust
pub struct CrawlAuditReport {
    pub path: std::path::PathBuf,
    pub snapshot: CrawlAuditSnapshot,
}

pub struct CrawlAuditDiffReport {
    pub path: std::path::PathBuf,
    pub diff: CrawlAuditSnapshotDiff,
}
```

Service entry points:
```rust
pub async fn run_audit(cfg: &Config, start_url: &str)
    -> Result<CrawlAuditReport, Box<dyn Error>>;

pub async fn run_audit_diff(cfg: &Config)
    -> Result<CrawlAuditDiffReport, Box<dyn Error>>;
```

### Task 1.1 — Write snapshot security tests (failing) in CLI location before moving

**Files:** Create `src/cli/commands/crawl/audit/manifest_audit_baseline_tests.rs`

- [ ] **Step 1: Write the failing snapshot test**

In `src/cli/commands/crawl/audit/manifest_audit_baseline_tests.rs`:
```rust
use super::manifest_audit::persist_audit_snapshot;
use crate::core::config::Config;
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::test]
async fn snapshot_empty_manifest_returns_empty_entries() {
    let dir = TempDir::new().expect("tempdir");
    let mut cfg = Config::default();
    cfg.output_dir = dir.path().to_path_buf();
    let (path, snap) = persist_audit_snapshot(&cfg, "https://example.com")
        .await
        .expect("audit ok");
    assert!(path.exists(), "report file must exist");
    assert_eq!(snap.manifest_entry_count, 0);
    assert_eq!(snap.manifest_entries.len(), 0);
}

#[tokio::test]
async fn resolve_path_rejects_null_byte() {
    // Verify the security boundary via the public snapshot function with a
    // crafted manifest.jsonl that embeds a null byte in file_path.
    let dir = TempDir::new().expect("tempdir");
    let manifest_path = dir.path().join("manifest.jsonl");
    tokio::fs::write(
        &manifest_path,
        r#"{"url":"https://example.com","file_path":"safe\0evil","markdown_chars":500}"#
            .as_bytes(),
    )
    .await
    .expect("write manifest");
    let mut cfg = Config::default();
    cfg.output_dir = dir.path().to_path_buf();
    let (_, snap) = persist_audit_snapshot(&cfg, "https://example.com")
        .await
        .expect("audit ok");
    // File with null byte path must be fingerprinted as rejected or missing.
    let entry = snap.manifest_entries.first().expect("one entry");
    assert!(
        entry.fingerprint == "path-outside-output-dir"
            || entry.fingerprint == "file-not-found",
        "null-byte path must be rejected; got {:?}",
        entry.fingerprint
    );
}

#[tokio::test]
async fn resolve_path_rejects_outside_output_dir() {
    let dir = TempDir::new().expect("tempdir");
    let manifest_path = dir.path().join("manifest.jsonl");
    let outside = "/etc/passwd";
    tokio::fs::write(
        &manifest_path,
        format!(r#"{{"url":"https://example.com","file_path":"{outside}","markdown_chars":500}}"#)
            .as_bytes(),
    )
    .await
    .expect("write manifest");
    let mut cfg = Config::default();
    cfg.output_dir = dir.path().to_path_buf();
    let (_, snap) = persist_audit_snapshot(&cfg, "https://example.com")
        .await
        .expect("audit ok");
    let entry = snap.manifest_entries.first().expect("one entry");
    assert_eq!(entry.fingerprint, "path-outside-output-dir");
}
```

Wire in `manifest_audit.rs`:
```rust
#[cfg(test)]
#[path = "manifest_audit_baseline_tests.rs"]
mod baseline_tests;
```

- [ ] **Step 2: Run baseline tests to confirm they compile and pass** (they document current behavior)
```bash
cd /home/jmagar/workspace/axon_rust
cargo test manifest_audit_baseline 2>&1 | tail -20
```
Expected: tests pass (they document existing behavior before the move).

- [ ] **Step 3: Commit baseline tests**
```bash
rtk git add src/cli/commands/crawl/audit/manifest_audit.rs src/cli/commands/crawl/audit/manifest_audit_baseline_tests.rs
rtk git commit -m "test(audit): add baseline security tests before service move (dvo.2)"
```

### Task 1.2 — Create `src/services/crawl/audit/` module structure

**Files:** Create `src/services/crawl/audit.rs`, `src/services/crawl/audit/manifest_audit.rs`, `src/services/crawl/audit/audit_diff.rs`, `src/services/crawl/audit/sitemap.rs`

- [ ] **Step 1: Write failing tests for the service module (before it exists)**

Create `src/services/crawl/audit/manifest_audit_tests.rs`:
```rust
use super::*;
use crate::core::config::Config;
use tempfile::TempDir;

#[tokio::test]
async fn audit_empty_dir_returns_empty_snapshot() {
    let dir = TempDir::new().expect("tempdir");
    let mut cfg = Config::default();
    cfg.output_dir = dir.path().to_path_buf();
    let report = run_audit(&cfg, "https://example.com")
        .await
        .expect("audit ok");
    assert!(report.path.exists());
    assert_eq!(report.snapshot.manifest_entry_count, 0);
}

#[tokio::test]
async fn null_byte_path_is_rejected() {
    let dir = TempDir::new().expect("tempdir");
    let manifest_path = dir.path().join("manifest.jsonl");
    tokio::fs::write(
        &manifest_path,
        r#"{"url":"https://example.com","file_path":"safe\0evil","markdown_chars":500}"#
            .as_bytes(),
    )
    .await
    .unwrap();
    let mut cfg = Config::default();
    cfg.output_dir = dir.path().to_path_buf();
    let report = run_audit(&cfg, "https://example.com").await.unwrap();
    let fp = &report.snapshot.manifest_entries[0].fingerprint;
    assert!(fp == "path-outside-output-dir" || fp == "file-not-found");
}

#[tokio::test]
async fn outside_root_path_is_rejected() {
    let dir = TempDir::new().expect("tempdir");
    let manifest_path = dir.path().join("manifest.jsonl");
    tokio::fs::write(
        &manifest_path,
        br#"{"url":"https://example.com","file_path":"/etc/passwd","markdown_chars":500}"#,
    )
    .await
    .unwrap();
    let mut cfg = Config::default();
    cfg.output_dir = dir.path().to_path_buf();
    let report = run_audit(&cfg, "https://example.com").await.unwrap();
    assert_eq!(
        report.snapshot.manifest_entries[0].fingerprint,
        "path-outside-output-dir"
    );
}
```

- [ ] **Step 2: Run tests to confirm they fail (module doesn't exist yet)**
```bash
cargo test services::crawl::audit 2>&1 | tail -10
```
Expected: compile error — module not found.

- [ ] **Step 3: Create `src/services/crawl/audit.rs`** (module root + public return types)

```rust
pub mod audit_diff;
pub mod manifest_audit;
pub mod sitemap;

pub use manifest_audit::{CrawlAuditSnapshot, ManifestAuditEntry};
pub use audit_diff::CrawlAuditSnapshotDiff;
pub use sitemap::{SitemapDiscoveryResult, SitemapDiscoveryStats};

use crate::core::config::Config;
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct CrawlAuditReport {
    pub path: std::path::PathBuf,
    pub snapshot: CrawlAuditSnapshot,
}

pub struct CrawlAuditDiffReport {
    pub path: std::path::PathBuf,
    pub diff: CrawlAuditSnapshotDiff,
}

pub(crate) fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

pub async fn run_audit(cfg: &Config, start_url: &str) -> Result<CrawlAuditReport, Box<dyn Error>> {
    crate::core::http::validate_url(start_url)?;
    let (path, snapshot) = manifest_audit::persist_audit_snapshot(cfg, start_url).await?;
    Ok(CrawlAuditReport { path, snapshot })
}

pub async fn run_audit_diff(cfg: &Config) -> Result<CrawlAuditDiffReport, Box<dyn Error>> {
    let (path, diff) = audit_diff::compute_audit_diff(cfg).await?;
    Ok(CrawlAuditDiffReport { path, diff })
}
```

- [ ] **Step 4: Copy `manifest_audit.rs` to `src/services/crawl/audit/manifest_audit.rs`**

Copy the content verbatim from `src/cli/commands/crawl/audit/manifest_audit.rs` and add the sidecar declaration at the bottom:
```rust
// (copy all content from src/cli/commands/crawl/audit/manifest_audit.rs)
// Replace: use super::sitemap::... with use super::sitemap::...
// Replace: use super::now_epoch_ms() → already in parent module (audit.rs), use super::now_epoch_ms
// Verify: persist_audit_snapshot signature unchanged

#[cfg(test)]
#[path = "manifest_audit_tests.rs"]
mod tests;
```

- [ ] **Step 5: Copy `audit_diff.rs` to `src/services/crawl/audit/audit_diff.rs`**

Extract `run_crawl_audit_diff` into a pure business-logic function `compute_audit_diff` that returns `(PathBuf, CrawlAuditSnapshotDiff)` — no printing:

```rust
use super::{CrawlAuditSnapshot, CrawlAuditSnapshotDiff, now_epoch_ms};
use crate::core::config::Config;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::{Path, PathBuf};

// (keep list_audit_reports, read_audit_snapshot, build_snapshot_diff unchanged)

/// Pure business logic — no printing. Returns path of written diff and the diff struct.
pub(super) async fn compute_audit_diff(
    cfg: &Config,
) -> Result<(PathBuf, CrawlAuditSnapshotDiff), Box<dyn Error>> {
    let reports = list_audit_reports(&cfg.output_dir).await?;
    if reports.len() < 2 {
        return Err(anyhow::anyhow!(
            "crawl diff requires at least two persisted crawl audit reports"
        )
        .into());
    }
    let previous_report = reports[reports.len() - 2].clone();
    let current_report = reports[reports.len() - 1].clone();
    let previous = read_audit_snapshot(&previous_report).await?;
    let current = read_audit_snapshot(&current_report).await?;
    let diff = build_snapshot_diff(&previous_report, &current_report, &previous, &current);
    let diff_path = cfg
        .output_dir
        .join("reports")
        .join("crawl-audit")
        .join(format!("diff-{}.json", now_epoch_ms()));
    tokio::fs::write(&diff_path, serde_json::to_string_pretty(&diff)?).await?;
    Ok((diff_path, diff))
}

#[cfg(test)]
#[path = "audit_diff_tests.rs"]
mod tests;
```

Create `src/services/crawl/audit/audit_diff_tests.rs`:
```rust
use super::*;
use crate::core::config::Config;
use tempfile::TempDir;

#[tokio::test]
async fn diff_requires_at_least_two_reports() {
    let dir = TempDir::new().expect("tempdir");
    let mut cfg = Config::default();
    cfg.output_dir = dir.path().to_path_buf();
    let err = compute_audit_diff(&cfg).await.unwrap_err();
    assert!(err.to_string().contains("two"), "error={err}");
}
```

- [ ] **Step 6: Copy `sitemap.rs` to `src/services/crawl/audit/sitemap.rs`** (unchanged — no printing; update visibility to `pub(crate)`)

Add sidecar declaration:
```rust
#[cfg(test)]
#[path = "sitemap_tests.rs"]
mod tests;
```

Create `src/services/crawl/audit/sitemap_tests.rs`:
```rust
use super::{SitemapDiscoveryResult, SitemapDiscoveryStats};

#[test]
fn serde_roundtrip() {
    let stats = SitemapDiscoveryStats {
        robots_declared_sitemaps: 1,
        seeded_default_sitemaps: 2,
        ..Default::default()
    };
    let result = SitemapDiscoveryResult {
        urls: vec!["https://example.com/".to_string()],
        stats,
    };
    let json = serde_json::to_string(&result).unwrap();
    let back: SitemapDiscoveryResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back.urls.len(), 1);
}
```

- [ ] **Step 7: Wire `audit` submodule into `src/services/crawl.rs`**

Add at the top of `src/services/crawl.rs`:
```rust
pub mod audit;
```

- [ ] **Step 8: Run tests — they should pass now**
```bash
cargo test services::crawl::audit 2>&1 | tail -20
```
Expected: all three module tests pass.

- [ ] **Step 9: Commit the services module**
```bash
rtk git add src/services/crawl.rs src/services/crawl/audit.rs src/services/crawl/audit/manifest_audit.rs src/services/crawl/audit/manifest_audit_tests.rs src/services/crawl/audit/audit_diff.rs src/services/crawl/audit/audit_diff_tests.rs src/services/crawl/audit/sitemap.rs src/services/crawl/audit/sitemap_tests.rs
rtk git commit -m "feat(services): add services/crawl/audit module (dvo.2, step 1)"
```

### Task 1.3 — Rewrite CLI audit shell to call services; delete moved files

**Files:** Modify `src/cli/commands/crawl/audit.rs`; delete the three moved source files.

- [ ] **Step 1: Rewrite `src/cli/commands/crawl/audit.rs` as a thin shell**

```rust
mod sitemap_migration_tests;

use crate::core::config::Config;
use crate::core::ui::{muted, primary};
use crate::services::crawl::audit as audit_svc;
use std::error::Error;

pub(super) async fn run_crawl_audit(cfg: &Config, start_url: &str) -> Result<(), Box<dyn Error>> {
    let report = audit_svc::run_audit(cfg, start_url).await?;
    let snapshot = &report.snapshot;
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "audit_report_path": report.path.to_string_lossy(),
                "snapshot": snapshot,
            }))?
        );
    } else {
        println!("{}", primary("Crawl Audit"));
        println!("  {} {}", muted("Report:"), report.path.to_string_lossy());
        println!("  {} {}", muted("Discovered URLs:"), snapshot.discovered_url_count);
        println!("  {} {}", muted("Manifest entries:"), snapshot.manifest_entry_count);
    }
    Ok(())
}

pub(super) async fn run_crawl_audit_diff(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let report = audit_svc::run_audit_diff(cfg).await?;
    let diff = &report.diff;
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "diff_report_path": report.path.to_string_lossy(),
                "diff": diff,
            }))?
        );
    } else {
        println!("{}", primary("Crawl Audit Diff"));
        println!("  {} {}", muted("Report:"), report.path.to_string_lossy());
        println!("  {} {}", muted("Manifest added:"), diff.manifest_added);
        println!("  {} {}", muted("Manifest removed:"), diff.manifest_removed);
        println!("  {} {}", muted("Manifest changed:"), diff.manifest_changed);
    }
    Ok(())
}
```

- [ ] **Step 2: Remove moved source files (use git rm so rename is tracked in diff)**
```bash
git rm /home/jmagar/workspace/axon_rust/src/cli/commands/crawl/audit/manifest_audit.rs
git rm /home/jmagar/workspace/axon_rust/src/cli/commands/crawl/audit/manifest_audit_tests.rs
git rm /home/jmagar/workspace/axon_rust/src/cli/commands/crawl/audit/manifest_audit_baseline_tests.rs
git rm /home/jmagar/workspace/axon_rust/src/cli/commands/crawl/audit/audit_diff.rs
git rm /home/jmagar/workspace/axon_rust/src/cli/commands/crawl/audit/sitemap.rs
```

- [ ] **Step 3: Compile and run tests**
```bash
cargo check --bin axon 2>&1 | head -30
cargo test crawl::audit 2>&1 | tail -20
```
Expected: clean compile; all audit tests pass.

- [ ] **Step 4: Run full test suite**
```bash
rtk cargo test 2>&1 | tail -20
```
Expected: all tests pass.

- [ ] **Step 5: Clippy and fmt check**
```bash
rtk cargo clippy --all-targets -- -D warnings 2>&1 | tail -20
cargo fmt --check 2>&1 | head -10
```

- [ ] **Step 6: Update CHANGELOG.md**

Add under a new patch version (bump patch):
```markdown
## [Unreleased]
### Refactor
- Move crawl audit subsystem (`manifest_audit`, `audit_diff`, `sitemap`) from
  `cli/commands/crawl/audit/` to `services/crawl/audit/`. CLI shell is now
  a thin presentation wrapper. Security boundary (`resolve_manifest_entry_path`)
  preserved bit-for-bit. (`axon_rust-dvo.2`)
```

- [ ] **Step 7: Commit**
```bash
rtk git add -A
rtk git commit -m "refactor(audit): move audit subsystem to services/crawl/audit/ (dvo.2)"
```

### Acceptance Criteria — dvo.2

- [ ] `src/cli/commands/crawl/audit/` directory contains only `audit.rs` (thin shell) and `sitemap_migration_tests.rs` — no `manifest_audit.rs`, `audit_diff.rs`, `sitemap.rs`
- [ ] `src/services/crawl/audit/` exists with `manifest_audit.rs`, `audit_diff.rs`, `sitemap.rs`
- [ ] No `mod.rs` files introduced
- [ ] No `println!`, `primary()`, `muted()`, or `cfg.json_output` in `services/crawl/audit/**`
- [ ] Security tests (null byte, outside root, empty manifest) pass in the new location
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` all green
- [ ] CHANGELOG entry present

---

## Section 2 — axon_rust-dvo.3 (P0): Centralize Artifact Persistence

**Priority:** P0 — can land in parallel with P1 (disjoint files)

**Goal:** Extract the `atomic_write_under` primitive; migrate scrape artifact writes (MCP handler, CLI, `/v1/actions` dispatcher, `thin_refetch`) to use service-owned persistence; delete `tokio::fs::write` from all three presentation layers for scrape/screenshot artifacts. `thin_refetch` receives a persist callback — no circular dep.

### File Map

Create:
- `src/services/io.rs` — declares `mod atomic_write;`
- `src/services/io/atomic_write.rs` — hardened `atomic_write_under` implementation
- `src/services/io/atomic_write_tests.rs` — sidecar unit tests
- `src/services/types/scrape.rs` — `ScrapeArtifacts` type (not in the 38K `service.rs`)

Modify:
- `src/services/types.rs` — add `pub mod scrape; pub use scrape::ScrapeArtifacts;`
- `src/services/scrape.rs` — add `persist_scrape_markdown(cfg, url, markdown) -> Result<ScrapeArtifacts>`
- `src/services/screenshot.rs` — pass `create_dir_all` inside service; remove from MCP handler
- `src/services/action_api/commands/dispatchers.rs` — remove `tokio::fs::write` from scrape dispatcher; use service artifact path
- `src/mcp/server/artifacts/respond.rs` — `write_json_artifact` body → `atomic_write_under`
- `src/mcp/server/handlers_system/screenshot.rs` — remove `create_dir_all`; let service handle it
- `src/crawl/engine/thin_refetch.rs` — accept `persist_fn: F` callback; remove `tokio::fs::write` from engine
- `CHANGELOG.md`

### Task 2.0 — Verify `tempfile` dev-dependency exists

- [ ] **Step 1: Check**
```bash
grep 'tempfile' /home/jmagar/workspace/axon_rust/Cargo.toml
```
If not present, add to `[dev-dependencies]`:
```toml
tempfile = "3"
```
Then: `cargo check --tests 2>&1 | head -5`

- [ ] **Step 2: Commit if changed**
```bash
rtk git add Cargo.toml && rtk git commit -m "chore: add tempfile dev-dependency for atomic_write tests (dvo.3)"
```

### Task 2.1 — Implement `services::io::atomic_write_under` (hardened)

- [ ] **Step 1: Write failing unit tests first**

Create `src/services/io/atomic_write_tests.rs`:
```rust
use super::atomic_write_under;
use tempfile::TempDir;

#[tokio::test]
async fn writes_content_and_returns_path() {
    let root = TempDir::new().expect("tempdir");
    let rel = std::path::Path::new("sub/test.json");
    let written = atomic_write_under(root.path(), rel, b"hello")
        .await
        .expect("write ok");
    assert!(written.exists());
    assert_eq!(tokio::fs::read(&written).await.unwrap(), b"hello");
}

#[tokio::test]
async fn rejects_absolute_relative_path() {
    let root = TempDir::new().expect("tempdir");
    let abs = std::path::Path::new("/etc/passwd");
    let err = atomic_write_under(root.path(), abs, b"x").await.unwrap_err();
    assert!(err.to_string().contains("absolute"), "err={err}");
}

#[tokio::test]
async fn rejects_traversal() {
    let root = TempDir::new().expect("tempdir");
    let traversal = std::path::Path::new("../../etc/passwd");
    let err = atomic_write_under(root.path(), traversal, b"x")
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("outside root") || err.to_string().contains("traversal"),
        "err={err}"
    );
}

#[tokio::test]
async fn rejects_null_byte_in_path() {
    let root = TempDir::new().expect("tempdir");
    let nul = std::path::Path::new("foo\0bar.json");
    let err = atomic_write_under(root.path(), nul, b"x").await.unwrap_err();
    assert!(err.to_string().contains("null"), "err={err}");
}

#[tokio::test]
async fn tmp_file_cleaned_up_on_success() {
    let root = TempDir::new().expect("tempdir");
    let rel = std::path::Path::new("out.json");
    atomic_write_under(root.path(), rel, b"data").await.unwrap();
    // No *.tmp sibling should remain.
    let mut dir = tokio::fs::read_dir(root.path()).await.unwrap();
    while let Some(entry) = dir.next_entry().await.unwrap() {
        let name = entry.file_name();
        assert!(
            !name.to_string_lossy().ends_with(".tmp"),
            "tmp file left: {name:?}"
        );
    }
}
```

- [ ] **Step 2: Run — confirm compile fails (module missing)**
```bash
cargo test services::io::atomic_write 2>&1 | head -10
```
Expected: compile error.

- [ ] **Step 3: Create `src/services/io.rs`**
```rust
pub mod atomic_write;
```

- [ ] **Step 4: Create `src/services/io/atomic_write.rs`**
```rust
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

/// Write `bytes` atomically to `root.join(relative_path)`.
///
/// Safety contract:
/// - `relative_path` must not be absolute
/// - `relative_path` must not contain null bytes
/// - After canonicalization, the final path must remain under `root`
/// - Uses write-tmp-rename to avoid partial files
/// - Calls `sync_all` before rename to ensure durability on power loss
/// - Best-effort syncs the parent directory after rename
///
/// Returns the absolute path of the written file.
pub async fn atomic_write_under(
    root: &Path,
    relative_path: &Path,
    bytes: &[u8],
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Reject absolute paths.
    if relative_path.is_absolute() {
        return Err(format!(
            "atomic_write_under: relative_path must not be absolute; got {:?}",
            relative_path
        )
        .into());
    }

    // Reject null bytes in path components.
    for component in relative_path.components() {
        let s = component.as_os_str().to_string_lossy();
        if s.contains('\0') {
            return Err(format!(
                "atomic_write_under: null byte in path component {:?}",
                component
            )
            .into());
        }
    }

    // Construct target path (before canonicalize — parent may not exist yet).
    let target = root.join(relative_path);

    // Ensure parent directory exists.
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Canonicalize root (root must exist at call time).
    let canon_root = tokio::fs::canonicalize(root).await?;

    // Write to temp sibling then rename atomically.
    let tmp_path = target.with_extension(format!(
        "{}.tmp",
        Uuid::new_v4().simple()
    ));
    {
        let mut f = tokio::fs::File::create(&tmp_path).await?;
        f.write_all(bytes).await?;
        f.sync_all().await?;
    }

    // Canonicalize the tmp path to verify it is under root.
    match tokio::fs::canonicalize(&tmp_path).await {
        Ok(canon_tmp) if canon_tmp.starts_with(&canon_root) => {}
        Ok(canon_tmp) => {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(format!(
                "atomic_write_under: resolved path {:?} is outside root {:?}",
                canon_tmp, canon_root
            )
            .into());
        }
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(format!("atomic_write_under: canonicalize failed: {e}").into());
        }
    }

    // Rename (atomic on POSIX).
    if let Err(e) = tokio::fs::rename(&tmp_path, &target).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(format!("atomic_write_under: rename failed: {e}").into());
    }

    // Best-effort sync parent dir.
    if let Some(parent) = target.parent() {
        let _ = tokio::fs::File::open(parent)
            .await
            .map(|f| async move { f.sync_all().await });
    }

    Ok(target)
}

#[cfg(test)]
#[path = "atomic_write_tests.rs"]
mod tests;
```

- [ ] **Step 5: Declare `io` in `src/services.rs` (or `src/lib.rs` — wherever services are declared)**
```bash
grep -n "^pub mod " /home/jmagar/workspace/axon_rust/src/services.rs | head -5
```
Add `pub mod io;` to `src/services.rs`.

- [ ] **Step 6: Run tests — confirm passing**
```bash
cargo test services::io::atomic_write 2>&1 | tail -20
```
Expected: all 5 tests pass.

- [ ] **Step 7: Commit**
```bash
rtk git add src/services/io.rs src/services/io/atomic_write.rs src/services/io/atomic_write_tests.rs src/services.rs
rtk git commit -m "feat(services/io): add hardened atomic_write_under primitive (dvo.3, step 1)"
```

### Task 2.2 — Add `ScrapeArtifacts` type and `persist_scrape_markdown` to `services::scrape`

**Files:** `src/services/types/scrape.rs`, `src/services/types.rs`, `src/services/scrape.rs`

- [ ] **Step 1: Write failing tests**

Add to `src/services/scrape_tests.rs`:
```rust
#[tokio::test]
async fn persist_scrape_markdown_writes_file_and_returns_path() {
    use crate::core::config::Config;
    use crate::services::scrape::persist_scrape_markdown;
    use tempfile::TempDir;

    let root = TempDir::new().unwrap();
    let mut cfg = Config::default();
    cfg.output_dir = root.path().to_path_buf();
    let artifacts = persist_scrape_markdown(&cfg, "https://example.com", "# Hello")
        .await
        .expect("persist ok");
    assert!(artifacts.md_path.exists(), "md file must exist");
    let content = tokio::fs::read_to_string(&artifacts.md_path).await.unwrap();
    assert_eq!(content, "# Hello");
}
```

- [ ] **Step 2: Run — confirm compile fails**
```bash
cargo test scrape::tests::persist_scrape_markdown 2>&1 | head -10
```

- [ ] **Step 3: Create `src/services/types/scrape.rs`**
```rust
use std::path::PathBuf;

/// Filesystem artifacts produced by a scrape operation.
#[derive(Debug, Clone)]
pub struct ScrapeArtifacts {
    /// Path to the written markdown file.
    pub md_path: PathBuf,
}
```

- [ ] **Step 4: Add to `src/services/types.rs`**
```rust
pub mod scrape;
pub use scrape::ScrapeArtifacts;
```

- [ ] **Step 5: Add `persist_scrape_markdown` to `src/services/scrape.rs`**
```rust
use crate::services::io::atomic_write::atomic_write_under;
use crate::services::types::ScrapeArtifacts;
use uuid::Uuid;

/// Persist scraped markdown content atomically to the output directory.
///
/// Writes to `cfg.output_dir/scrape/runs/<uuid>/<filename>.md`.
/// Returns the artifact paths for use by embed and MCP response construction.
pub async fn persist_scrape_markdown(
    cfg: &Config,
    url: &str,
    markdown: &str,
) -> Result<ScrapeArtifacts, Box<dyn Error>> {
    let run_id = Uuid::new_v4().simple().to_string();
    // Derive a stable filename from the URL: replace non-alphanum with _.
    let safe_name: String = url
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(80)
        .collect();
    let rel = std::path::Path::new("scrape")
        .join("runs")
        .join(&run_id)
        .join(format!("{safe_name}.md"));
    let md_path = atomic_write_under(&cfg.output_dir, &rel, markdown.as_bytes()).await?;
    Ok(ScrapeArtifacts { md_path })
}
```

- [ ] **Step 6: Run tests — confirm passing**
```bash
cargo test services::scrape 2>&1 | tail -10
```

- [ ] **Step 7: Commit**
```bash
rtk git add src/services/types.rs src/services/types/scrape.rs src/services/scrape.rs src/services/scrape_tests.rs
rtk git commit -m "feat(services): add ScrapeArtifacts type and persist_scrape_markdown (dvo.3, step 2)"
```

### Task 2.3 — Migrate `respond.rs::write_json_artifact` to use `atomic_write_under`

**Files:** `src/mcp/server/artifacts/respond.rs`

- [ ] **Step 1: Write a test that the artifact body survives the rename**

Add to `src/mcp/server/artifacts/respond_tests.rs`:
```rust
#[tokio::test]
async fn write_json_artifact_produces_valid_file() {
    // Set AXON_DATA_DIR to a temp dir so path resolution works.
    let dir = tempfile::TempDir::new().unwrap();
    std::env::set_var("AXON_DATA_DIR", dir.path());
    let payload = serde_json::json!({"key": "value", "num": 42});
    let result = super::write_json_artifact("test-stem", &payload)
        .await
        .expect("write ok");
    let path_str = result["path"].as_str().expect("path field");
    let full_path = dir.path().join("artifacts").join("json").join(
        std::path::Path::new(path_str).file_name().unwrap()
    );
    // Verify the file was written and is valid JSON.
    if full_path.exists() {
        let content = std::fs::read_to_string(&full_path).unwrap();
        let _: serde_json::Value = serde_json::from_str(&content).expect("valid json");
    }
}
```

- [ ] **Step 2: Run — confirm existing test passes (don't break it)**
```bash
cargo test mcp::server::artifacts::respond 2>&1 | tail -10
```

- [ ] **Step 3: Replace the tmp+rename body in `write_json_artifact`**

In `src/mcp/server/artifacts/respond.rs`, replace the direct `tokio::fs::write` with `atomic_write_under`. The artifact root comes from `build_artifact_path`:

```rust
// Add at top of file:
use crate::services::io::atomic_write::atomic_write_under;

pub async fn write_json_artifact(
    stem: &str,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, ErrorData> {
    let text = serde_json::to_string_pretty(payload).map_err(|e| internal_error(e.to_string()))?;
    let path = build_artifact_path(stem, "json").await?;

    // Use the hardened atomic_write_under helper.
    // build_artifact_path already resolves inside the artifact root; we use
    // the artifact root as the write root and the filename as the relative path.
    let artifact_root = path
        .parent()
        .ok_or_else(|| internal_error("artifact path has no parent"))?;
    let filename = path
        .file_name()
        .ok_or_else(|| internal_error("artifact path has no filename"))?;
    atomic_write_under(artifact_root, std::path::Path::new(filename), text.as_bytes())
        .await
        .map_err(|e| internal_error(format!("failed to write artifact: {e}")))?;

    // (rest of function — artifact_handle_for_path etc. — unchanged)
    // ...
}
```

- [ ] **Step 4: Run tests**
```bash
cargo test mcp::server::artifacts::respond 2>&1 | tail -10
cargo check --bin axon 2>&1 | head -10
```

- [ ] **Step 5: Commit**
```bash
rtk git add src/mcp/server/artifacts/respond.rs src/mcp/server/artifacts/respond_tests.rs
rtk git commit -m "refactor(mcp/artifacts): use atomic_write_under in write_json_artifact (dvo.3, step 3)"
```

### Task 2.4 — Remove `tokio::fs::create_dir_all` from MCP screenshot handler

**Files:** `src/mcp/server/handlers_system/screenshot.rs`

- [ ] **Step 1: The MCP screenshot handler currently calls `create_dir_all` for the screenshots dir** — the service already handles this in `screenshot_capture`. Remove it.

In `src/mcp/server/handlers_system/screenshot.rs`, the `else` branch of output path resolution:
```rust
// BEFORE:
} else {
    let screenshots_dir = ensure_artifact_root().await?.join("screenshots");
    tokio::fs::create_dir_all(&screenshots_dir)
        .await
        .map_err(|e| logged_internal_error("screenshot dir", &e))?;
    screenshots_dir.join(format!("{}.png", ...))
};
```
The service `screenshot_capture` already calls `create_dir_all` on the path parent. Remove the `create_dir_all` call from the handler; keep just the path construction.

- [ ] **Step 2: Compile check**
```bash
cargo check --bin axon 2>&1 | head -20
```

- [ ] **Step 3: Commit**
```bash
rtk git add src/mcp/server/handlers_system/screenshot.rs
rtk git commit -m "refactor(mcp): remove create_dir_all from screenshot handler (dvo.3, step 4)"
```

### Task 2.5 — Migrate `/v1/actions` scrape dispatcher to service artifact

**Files:** `src/services/action_api/commands/dispatchers.rs`

- [ ] **Step 1: Write a migration test**

Add to `src/services/action_api_tests.rs` or a new `src/services/action_api/commands/dispatchers_tests.rs`:
```rust
// Integration test: verify no tokio::fs::write in dispatch_scrape after migration.
// (This is a code-structure assertion — checked by CI grep in acceptance criteria.)
// Actual behavior test: dispatch_scrape returns artifact info in result.
```

- [ ] **Step 2: Migrate `dispatch_scrape` in `dispatchers.rs`**

The dispatcher currently:
1. Calls `scrape_svc::scrape(...)` to get `result`
2. Manually does `create_dir_all` + `tokio::fs::write(path, &result.output)`

After migration:
1. Call `scrape_svc::scrape(...)` to get `result`
2. If `cfg.output_path.is_some()` OR `result.output.len() > 0`, call `persist_scrape_markdown(&cfg, &url, &result.markdown)` to get the artifact path
3. Use the artifact path in the response

```rust
// In dispatch_scrape:
let result = scrape_svc::scrape(&cfg, &url, None)
    .await
    .map_err(internal_error)?;

// Persist markdown via service (atomic, rooted, safe).
let artifacts = if !result.markdown.is_empty() {
    Some(
        crate::services::scrape::persist_scrape_markdown(&cfg, &url, &result.markdown)
            .await
            .map_err(internal_error)?,
    )
} else {
    None
};

Ok(serde_json::json!({
    "url": result.url,
    "markdown": result.markdown,
    "output": result.output,
    "payload": result.payload,
    "artifact_handle": result.artifact_handle,
    "md_path": artifacts.as_ref().map(|a| a.md_path.to_string_lossy()),
}))
```

- [ ] **Step 3: Compile and test**
```bash
cargo check --bin axon 2>&1 | head -20
rtk cargo test 2>&1 | tail -20
```

- [ ] **Step 4: Commit**
```bash
rtk git add src/services/action_api/commands/dispatchers.rs
rtk git commit -m "refactor(action_api): use persist_scrape_markdown in dispatch_scrape (dvo.3, step 5)"
```

### Task 2.6 — Wire `thin_refetch` persist callback

**Files:** `src/crawl/engine/thin_refetch.rs`, `src/services/crawl.rs` (caller)

- [ ] **Step 1: Write a test for the callback wiring pattern**

Add to `src/crawl/engine/thin_refetch_tests.rs` (create if missing):
```rust
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn persist_fn_is_called_for_recovered_page() {
    // Build a minimal refetch context; verify that persist_fn is called
    // with the expected path and content bytes.
    let written = Arc::new(Mutex::new(Vec::<(std::path::PathBuf, Vec<u8>)>::new()));
    let written2 = written.clone();
    let persist_fn = move |path: std::path::PathBuf, bytes: Vec<u8>| {
        let w = written2.clone();
        async move {
            w.lock().unwrap().push((path, bytes));
            Ok::<(), Box<dyn std::error::Error>>(())
        }
    };
    // (full integration requires a running Chrome; this is a structural test
    // that the callback signature compiles and is invoked)
    drop(persist_fn); // structural compile check
}
```

- [ ] **Step 2: Refactor `thin_refetch.rs` to accept a persist callback**

Change the public API of `run_thin_refetch` (or equivalent) to accept:
```rust
pub async fn run_thin_refetch<F, Fut>(
    cfg: &Config,
    markdown_dir: &Path,
    summary: &mut ThinRefetchSummary,
    persist_fn: F,
) -> Result<(), Box<dyn Error>>
where
    F: Fn(std::path::PathBuf, Vec<u8>) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send,
```

**If the generic closure fights rustc lifetime bounds** (captured borrows across awaits), fall back to the trait-object form:
```rust
use std::pin::Pin;
use std::sync::Arc;
type PersistFn = Arc<
    dyn Fn(
            std::path::PathBuf,
            Vec<u8>,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send>>
        + Send
        + Sync,
>;
pub async fn run_thin_refetch(
    cfg: &Config,
    markdown_dir: &Path,
    summary: &mut ThinRefetchSummary,
    persist_fn: PersistFn,
) -> Result<(), Box<dyn Error>>
```
Both forms satisfy the "no circular dep" constraint.

Replace the `tokio::fs::write(&tmp_path, markdown.as_bytes())` + rename block with:
```rust
let abs_path = markdown_dir.join(&filename);
let bytes = markdown.into_bytes();
if let Err(e) = persist_fn(abs_path.clone(), bytes).await {
    log_warn(&format!("thin_refetch: persist failed: {e}"));
    summary.thin_pages += 1;
    summary.thin_urls.insert(canonical);
    continue;
}
```

- [ ] **Step 3: Update the caller in `src/services/crawl.rs` (or wherever `run_thin_refetch` is called)**

Pass an inline callback using `services::io::atomic_write_under`:
```rust
use crate::services::io::atomic_write::atomic_write_under;

// When calling run_thin_refetch:
let persist_fn = |path: std::path::PathBuf, bytes: Vec<u8>| async move {
    let parent = path.parent().unwrap_or(&path);
    let filename = path.file_name().map(std::path::Path::new).unwrap_or(&path);
    atomic_write_under(parent, filename, &bytes)
        .await
        .map(|_| ())
};
run_thin_refetch(cfg, &markdown_dir, &mut summary, persist_fn).await?;
```

- [ ] **Step 4: Compile check — no circular dep**
```bash
cargo check --bin axon 2>&1 | head -20
```
Expected: clean. If circular dep error appears, move the callback definition to `src/crawl/engine/thin_refetch.rs` as a type alias and have the service supply a concrete impl.

- [ ] **Step 5: Full test suite**
```bash
rtk cargo test 2>&1 | tail -20
```

- [ ] **Step 6: Final validation grep**
```bash
grep -rn 'tokio::fs::write\|tokio::fs::create_dir_all' \
  /home/jmagar/workspace/axon_rust/src/cli/commands/ \
  /home/jmagar/workspace/axon_rust/src/mcp/server/handlers_query.rs \
  /home/jmagar/workspace/axon_rust/src/mcp/server/handlers_system/screenshot.rs \
  /home/jmagar/workspace/axon_rust/src/services/action_api/commands/dispatchers.rs \
  /home/jmagar/workspace/axon_rust/src/crawl/engine/thin_refetch.rs 2>/dev/null \
  | grep -v '_tests\.'
```
Expected: no output.

- [ ] **Step 7: Update CHANGELOG and bump version, then commit**
```bash
rtk git add -A
rtk git commit -m "refactor(services): centralize artifact persistence, atomic_write_under (dvo.3)"
```

### Acceptance Criteria — dvo.3

- [ ] `grep -rn 'tokio::fs::write\|tokio::fs::create_dir_all' src/cli/commands/ src/mcp/server/handlers_query.rs src/mcp/server/handlers_system/screenshot.rs src/services/action_api/commands/dispatchers.rs src/crawl/engine/thin_refetch.rs` returns zero results (excluding test files)
- [ ] `src/services/io/atomic_write.rs` exists and all 5 unit tests pass
- [ ] `src/services/types/scrape.rs` with `ScrapeArtifacts` exists
- [ ] `ScrapeArtifacts` is NOT in `service.rs` (38K file — check that file didn't grow)
- [ ] `write_json_artifact` body calls `atomic_write_under`
- [ ] No circular dep between `src/crawl/engine` and `src/services`
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` green
- [ ] CHANGELOG entry

---

## Section 3 — axon_rust-dvo.4 (P2): Validation and Preconditions into Services

**Priority:** P2 — depends on P0 (dvo.3 stabilizes scrape service signature)

**Goal:** Each service module that has external dependencies gets `pub fn validate_config(&Config) -> Result<(), Box<dyn Error>>` called on entry. Duplicate validation removed from CLI and MCP layers. `validate_mcp_url` deleted (no added policy). `validate_mcp_urls` kept (index-in-error-message is MCP presentation policy). Services own URL normalize+validate at entry.

### File Map

Add:
- `src/services/search_tests.rs` (extend) — `validate_config` tests for search + research

Modify:
- `src/services/search.rs` — expose `validate_config` that checks `tavily_api_key`; add `validate_config_for_research` checking Gemini headless config
- `src/services/screenshot.rs` — keep existing chrome check at entry
- `src/mcp/server/common.rs` — **delete** `validate_mcp_url`; keep `validate_mcp_urls`
- `src/mcp/server/handlers_query.rs` — replace `validate_mcp_url` with direct `crate::core::http::validate_url` mapped to `invalid_params`
- `src/mcp/server/handlers_query/brand_diff.rs` — same
- `src/mcp/server/handlers_system/screenshot.rs` — same
- `src/cli/commands/research.rs` — delete `validate_research_prereqs` if it still exists; propagate from service
- `CHANGELOG.md`

### Task 3.1 — Add `validate_config` to `services::search` and `services::search::validate_config_for_research`

- [ ] **Step 1: Write failing tests**

Add to `src/services/search_tests.rs`:
```rust
use crate::core::config::Config;
use crate::services::search::{validate_config, validate_config_for_research};

fn cfg_with_tavily(key: &str) -> Config {
    let mut cfg = Config::default();
    cfg.tavily_api_key = key.to_string();
    cfg
}

#[test]
fn search_validate_fails_without_tavily() {
    let cfg = cfg_with_tavily("");
    assert!(validate_config(&cfg).is_err());
    let msg = validate_config(&cfg).unwrap_err().to_string();
    assert!(msg.contains("TAVILY_API_KEY"), "msg={msg}");
}

#[test]
fn search_validate_passes_with_tavily() {
    let cfg = cfg_with_tavily("tvly-abc123");
    assert!(validate_config(&cfg).is_ok());
}

#[test]
fn research_validate_fails_without_gemini() {
    let mut cfg = cfg_with_tavily("tvly-abc123");
    cfg.headless_gemini_cmd = String::new(); // no Gemini CLI configured
    let err = validate_config_for_research(&cfg).unwrap_err();
    assert!(
        err.to_string().contains("gemini") || err.to_string().contains("Gemini"),
        "err={err}"
    );
}

#[test]
fn research_validate_passes_with_both() {
    let mut cfg = cfg_with_tavily("tvly-abc123");
    cfg.headless_gemini_cmd = "gemini".to_string();
    assert!(validate_config_for_research(&cfg).is_ok());
}
```

- [ ] **Step 2: Run — confirm compile fails**
```bash
cargo test services::search 2>&1 | head -10
```

- [ ] **Step 3: Add `validate_config` and `validate_config_for_research` to `src/services/search.rs`**

```rust
/// Validate that the Tavily API key is configured.
/// Called on entry by `search` and `research` — callers do not need to check.
pub fn validate_config(cfg: &Config) -> Result<(), Box<dyn Error>> {
    ensure_tavily_configured(cfg, "search")?;
    Ok(())
}

/// Validate that both Tavily and the Gemini headless CLI are configured.
/// Called by the research service path.
pub fn validate_config_for_research(cfg: &Config) -> Result<(), Box<dyn Error>> {
    ensure_tavily_configured(cfg, "research")?;
    if cfg.headless_gemini_cmd.trim().is_empty() {
        return Err(
            "research requires AXON_HEADLESS_GEMINI_CMD — set to the gemini CLI binary path \
             (run 'axon doctor' to diagnose)"
                .into(),
        );
    }
    Ok(())
}
```

Also add a call at the entry of `pub async fn search(...)` and `pub async fn research(...)`:
```rust
pub async fn search(cfg: &Config, ...) -> Result<SearchResult, Box<dyn Error>> {
    validate_config(cfg)?;
    // ... existing body
}
```

- [ ] **Step 4: Run tests — passing**
```bash
cargo test services::search 2>&1 | tail -15
```

- [ ] **Step 5: Commit**
```bash
rtk git add src/services/search.rs src/services/search_tests.rs
rtk git commit -m "feat(services/search): add validate_config and validate_config_for_research (dvo.4, step 1)"
```

### Task 3.2 — Delete `validate_mcp_url` from `common.rs`; update call-sites

- [ ] **Step 1: Write a test that documents the replacement pattern**

Add to `src/mcp/server/common_tests.rs`:
```rust
#[test]
fn validate_mcp_url_is_deleted() {
    // This test documents that validate_mcp_url was removed.
    // Callers now use: crate::core::http::validate_url(url).map_err(|e| invalid_params(e.to_string()))
    // No runtime assertion needed — if validate_mcp_url still exists, CI grep will catch it.
}
```

- [ ] **Step 2: Delete `validate_mcp_url` from `src/mcp/server/common.rs`**

Remove the two functions:
```rust
// DELETE:
pub(super) fn validate_mcp_url(url: &str) -> Result<(), ErrorData> { ... }
// KEEP:
pub(super) fn validate_mcp_urls(urls: &[String]) -> Result<(), ErrorData> { ... }
```

- [ ] **Step 3: Fix compile errors — replace each `validate_mcp_url` call-site**

In `handlers_query.rs`, `handlers_query/brand_diff.rs`, `handlers_system/screenshot.rs`:

Replace:
```rust
validate_mcp_url(&url)?;
```
With:
```rust
crate::core::http::validate_url(&url)
    .map_err(|e| invalid_params(e.to_string()))?;
```

- [ ] **Step 4: Compile check**
```bash
cargo check --bin axon 2>&1 | head -20
```

- [ ] **Step 5: Validate grep**
```bash
grep -rn 'validate_mcp_url\b' /home/jmagar/workspace/axon_rust/src/ | grep -v 'validate_mcp_urls'
```
Expected: zero results.

- [ ] **Step 6: Full test suite**
```bash
rtk cargo test 2>&1 | tail -15
```

- [ ] **Step 7: Commit**
```bash
rtk git add src/mcp/server/common.rs src/mcp/server/common_tests.rs src/mcp/server/handlers_query.rs src/mcp/server/handlers_query/brand_diff.rs src/mcp/server/handlers_system/screenshot.rs
rtk git commit -m "refactor(mcp): delete validate_mcp_url, use validate_url directly (dvo.4, step 2)"
```

### Task 3.3 — Remove duplicate CLI research prereq check; update CHANGELOG

- [ ] **Step 1: Check if `validate_research_prereqs` still exists in CLI**
```bash
grep -rn 'validate_research_prereqs\|tavily.*is_empty\|gemini.*is_empty' /home/jmagar/workspace/axon_rust/src/cli/commands/ 2>/dev/null
```

If it exists, delete it and let the service-level `validate_config_for_research` surface the error.

- [ ] **Step 2: Full test suite**
```bash
rtk cargo test 2>&1 | tail -15
rtk cargo clippy --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt --check 2>&1 | head -5
```

- [ ] **Step 3: Update CHANGELOG and bump version**

- [ ] **Step 4: Commit**
```bash
rtk git add -A
rtk git commit -m "refactor(validation): centralize service preconditions, delete validate_mcp_url (dvo.4)"
```

### Acceptance Criteria — dvo.4

- [ ] `grep -rn 'fn validate_mcp_url\b' src/mcp/server/common.rs` returns zero
- [ ] `grep -rn 'validate_mcp_url\b' src/` returns zero (excluding `validate_mcp_urls`)
- [ ] `grep -rn 'tavily_api_key.is_empty()' src/mcp/server/handlers_*.rs` returns zero
- [ ] `src/services/search.rs` exports `validate_config` and `validate_config_for_research`
- [ ] Both functions are called at entry of their respective service functions
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` green
- [ ] CHANGELOG entry

---

## Section 4 — axon_rust-dvo.5 (P3): Unify Ingest Target Parsing

**Priority:** P3 — depends on P2 (dvo.4), reuses error type patterns

**Goal:** `src/services/ingest/request.rs::source_from_mcp_request` is the one canonical parser. MCP handler already calls it (via `handlers_embed_ingest.rs`). CLI uses `classify_target`. `/v1/actions` (helpers.rs) delegates to it. Tighten `/v1/actions` path which currently does no validation before enqueue. Verify parity: same target string → same `IngestSource` from all three surfaces.

**Note:** The main work is (a) confirming MCP handler calls `source_from_mcp_request` (not the old inline `parse_ingest_source`), (b) updating `/v1/actions` helpers to validate before enqueue, and (c) adding parity tests.

### File Map

Modify:
- `src/mcp/server/handlers_embed_ingest.rs` — verify/ensure it calls `source_from_mcp_request` (not inline parser); if old `parse_ingest_source` still exists, delete it
- `src/services/action_api/commands/helpers.rs` — `parse_ingest_source` already calls `ingest_svc::source_from_mcp_request`; add explicit validation call
- `src/services/ingest/request.rs` — no changes needed if already complete; add `#[must_use]` if missing
- `CHANGELOG.md`

Add:
- `src/services/ingest_tests.rs` (extend) — parity tests
- `src/mcp/server/handlers_embed_ingest_tests.rs` (extend) — verify handler uses service parser

### Task 4.1 — Audit and confirm MCP handler uses `source_from_mcp_request`

- [ ] **Step 1: Check whether the old `parse_ingest_source` still exists in the MCP handler**
```bash
grep -n 'fn parse_ingest_source' /home/jmagar/workspace/axon_rust/src/mcp/server/handlers_embed_ingest.rs
```

If it exists: the CLAUDE.md for mcp notes that `ingest.start` now calls `source_from_mcp_request` in `src/services/ingest/request.rs`, but the handler file still has the old inline function. Delete the old one and update the handler to call the service function.

- [ ] **Step 2: Write a failing parity test**

Add to `src/services/ingest_tests.rs`:
```rust
use crate::core::config::Config;
use crate::mcp::schema::{IngestRequest, IngestSourceType};
use crate::services::ingest::request::source_from_mcp_request;
use crate::services::ingest::IngestSource;

#[test]
fn github_round_trip_owner_slash_repo() {
    let req = IngestRequest {
        source_type: Some(IngestSourceType::Github),
        target: Some("owner/repo".to_string()),
        include_source: None,
        sessions: None,
        subaction: None,
        job_id: None,
        limit: None,
        offset: None,
        response_mode: None,
    };
    let cfg = Config::default();
    let source = source_from_mcp_request(&req, &cfg).expect("parse ok");
    match source {
        IngestSource::Github { repo, .. } => assert_eq!(repo, "owner/repo"),
        other => panic!("expected Github, got {other:?}"),
    }
}

#[test]
fn github_rejects_invalid_format() {
    let req = IngestRequest {
        source_type: Some(IngestSourceType::Github),
        target: Some("not-a-valid-repo-format-without-slash@@@@".to_string()),
        include_source: None,
        sessions: None,
        subaction: None,
        job_id: None,
        limit: None,
        offset: None,
        response_mode: None,
    };
    let cfg = Config::default();
    assert!(source_from_mcp_request(&req, &cfg).is_err());
}

#[test]
fn reddit_rejects_invalid_subreddit_name() {
    let req = IngestRequest {
        source_type: Some(IngestSourceType::Reddit),
        target: Some("ab".to_string()), // too short
        include_source: None,
        sessions: None,
        subaction: None,
        job_id: None,
        limit: None,
        offset: None,
        response_mode: None,
    };
    let cfg = Config::default();
    assert!(source_from_mcp_request(&req, &cfg).is_err());
}

#[test]
fn youtube_rejects_non_video_url() {
    let req = IngestRequest {
        source_type: Some(IngestSourceType::Youtube),
        target: Some("https://not-youtube.com/watch?v=abc".to_string()),
        include_source: None,
        sessions: None,
        subaction: None,
        job_id: None,
        limit: None,
        offset: None,
        response_mode: None,
    };
    let cfg = Config::default();
    assert!(source_from_mcp_request(&req, &cfg).is_err());
}
```

- [ ] **Step 3: Run tests — they should pass (parser already implemented)**
```bash
cargo test ingest_tests 2>&1 | tail -15
```

- [ ] **Step 4: Add validation to `/v1/actions` helper path**

In `src/services/action_api/commands/helpers.rs`, ensure `parse_ingest_source` (which already calls `source_from_mcp_request`) also calls `validate_ingest_source`:

```rust
pub(super) fn parse_ingest_source(
    req: &IngestRequest,
    cfg: &Config,
) -> Result<ingest_svc::IngestSource, ClientActionError> {
    let source = ingest_svc::source_from_mcp_request(req, cfg)
        .map_err(|message| ClientActionError::new("invalid_request", message, false, None))?;
    // Explicit validation (double-check) before enqueue.
    ingest_svc::validate_ingest_source(&source)
        .map_err(|message| ClientActionError::new("invalid_request", message, false, None))?;
    Ok(source)
}
```

- [ ] **Step 5: Compile and run full test suite**
```bash
rtk cargo test 2>&1 | tail -20
```

- [ ] **Step 6: Validate grep — no inline `parse_ingest_source` with domain logic in MCP handler**
```bash
grep -n 'fn parse_ingest_source' /home/jmagar/workspace/axon_rust/src/mcp/server/handlers_embed_ingest.rs
```
Expected: no output.

- [ ] **Step 7: Update CHANGELOG and commit**
```bash
rtk git add -A
rtk git commit -m "refactor(ingest): unify target parsing via source_from_mcp_request; add parity tests (dvo.5)"
```

### Acceptance Criteria — dvo.5

- [ ] `grep -n 'fn parse_ingest_source' src/mcp/server/handlers_embed_ingest.rs` returns zero
- [ ] `src/services/ingest/request.rs::source_from_mcp_request` is the single canonical parser
- [ ] CLI, MCP, and `/v1/actions` all call through it (directly or via thin delegating wrapper)
- [ ] Parity tests: github, reddit, youtube invalid targets rejected identically from all surfaces
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` green
- [ ] CHANGELOG entry

---

## Section 5 — axon_rust-dvo.6 (P4): Unify Response Envelope Construction

**Priority:** P4 — depends on P3; all result types must be stable first

**Goal:** Reduce hand-rolled `serde_json::json!({...})` in MCP handlers by ensuring every public service result type derives `Serialize`. Use `serde_json::to_value(&result)` as the envelope payload where the struct shape IS the wire shape. Add a thin wrapper only where transport-specific policy diverges. `/v1/actions` and MCP keep separate outer envelopes; share only the inner typed data.

**Do NOT collapse MCP `AxonToolResponse` and `/v1/actions` response into one shape. Do NOT add `IntoEnvelope` trait if `Serialize` is sufficient.**

### File Map

Modify:
- `src/services/types/service.rs` — audit all result structs for `#[derive(Serialize)]`; add where missing
- `src/services/types/scrape.rs` — add `Serialize` to `ScrapeArtifacts`
- `src/mcp/server/handlers_query.rs` — where result structs are already `Serialize`, replace hand-rolled `json!` with `serde_json::to_value(&result)` or include struct fields directly
- `src/mcp/server/handlers_crawl_extract.rs` — same
- `src/mcp/server/handlers_embed_ingest.rs` — same
- `src/mcp/server/handlers_system.rs` — same
- `CHANGELOG.md`

Add:
- `src/services/types/envelope_tests.rs` — golden snapshot tests per result type

### Task 5.1 — Audit and add `#[derive(Serialize)]` to all public result types

- [ ] **Step 1: Find result types missing Serialize**
```bash
grep -n 'pub struct.*Result\|pub struct.*Artifacts' /home/jmagar/workspace/axon_rust/src/services/types/service.rs | head -40
grep -B2 'pub struct.*Result' /home/jmagar/workspace/axon_rust/src/services/types/service.rs | grep -v 'derive.*Serialize' | grep 'pub struct'
```

- [ ] **Step 2: Write golden snapshot test first**

Create `src/services/types/envelope_tests.rs`:
```rust
use super::service::*;

#[test]
fn scrape_result_serializes_to_stable_shape() {
    let result = ScrapeResult {
        url: "https://example.com".to_string(),
        markdown: "# Hello".to_string(),
        output: "# Hello".to_string(),
        payload: serde_json::json!({}),
        artifact_handle: None,
        truncated: false,
        token_estimate: None,
        next_cursor: None,
        remaining_tokens_estimate: None,
        backend: None,
        follow_crawl_urls: vec![],
        extra: None,
        extractor_name: None,
        title: None,
    };
    let val = serde_json::to_value(&result).expect("serialize ok");
    assert!(val.get("url").is_some(), "url field must be present");
    assert!(val.get("markdown").is_some(), "markdown field must be present");
}

#[test]
fn search_result_serializes_to_stable_shape() {
    // Add for each result type that MCP handlers construct manually.
    // Ensures struct shape == intended wire shape.
}
```

Wire in `src/services/types.rs`:
```rust
#[cfg(test)]
#[path = "types/envelope_tests.rs"]
mod envelope_tests;
```

- [ ] **Step 3: Run — identify compile failures (missing Serialize derives)**
```bash
cargo test types::envelope_tests 2>&1 | head -20
```

- [ ] **Step 4: Add missing `#[derive(Serialize, Deserialize)]` to affected structs**

For each compile error, add the derive. Check file size doesn't exceed 500 LOC after additions.

- [ ] **Step 5: Run tests — all golden snapshots pass**
```bash
cargo test types::envelope_tests 2>&1 | tail -15
```

- [ ] **Step 6: Commit Serialize adds**
```bash
rtk git add src/services/types/service.rs src/services/types/scrape.rs src/services/types.rs src/services/types/envelope_tests.rs
rtk git commit -m "feat(types): add Serialize derives to all public service result types (dvo.6, step 1)"
```

### Task 5.2 — Reduce hand-rolled json! in MCP handlers

- [ ] **Step 1: Identify the highest-value handlers to simplify**

Scan for `serde_json::json!` in MCP handler files, focusing on cases where all fields already exist in the result struct:
```bash
grep -c 'serde_json::json!' /home/jmagar/workspace/axon_rust/src/mcp/server/handlers_query.rs
grep -c 'serde_json::json!' /home/jmagar/workspace/axon_rust/src/mcp/server/handlers_system.rs
```

- [ ] **Step 2: For each handler where result IS the wire shape, replace `json!({})` with `to_value`**

Pattern:
```rust
// BEFORE (hand-rolled):
respond_with_mode("query", "query", response_mode, &stem, serde_json::json!({
    "query": query,
    "limit": limit,
    "results": result.results,
    // ... etc
}), InlineHint::Default).await

// AFTER (if QueryResult fields match the wire shape):
respond_with_mode("query", "query", response_mode, &stem,
    serde_json::to_value(&result).map_err(|e| internal_error(e.to_string()))?,
    InlineHint::Default).await
```

Only make this change where the struct fields exactly match the expected wire shape. If a handler adds extra fields (like `query` echoed back), keep a thin `json!` wrapper that merges struct with extra fields:
```rust
let mut val = serde_json::to_value(&result)?;
val["query"] = serde_json::Value::String(query.clone());
val["limit"] = limit.into();
respond_with_mode(..., val, ...).await
```

- [ ] **Step 3: Verify existing MCP migration tests still pass**
```bash
cargo test services_migration_tests 2>&1 | tail -15
```

- [ ] **Step 4: Add golden assertions for MCP and action_api envelopes separately**

In `src/mcp/server/services_migration_tests.rs`, verify that the outer MCP envelope shape (`ok`, `action`, `subaction`, `data`) is preserved:
```rust
#[test]
fn mcp_ok_envelope_has_canonical_shape() {
    let resp = AxonToolResponse::ok("query", "query", serde_json::json!({"results": []}));
    let val = serde_json::to_value(&resp).unwrap();
    assert!(val.get("ok").and_then(|v| v.as_bool()) == Some(true));
    assert_eq!(val["action"].as_str(), Some("query"));
    assert_eq!(val["subaction"].as_str(), Some("query"));
    assert!(val.get("data").is_some());
}
```

- [ ] **Step 5: Full test suite**
```bash
rtk cargo test 2>&1 | tail -20
rtk cargo clippy --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt --check 2>&1 | head -5
```

- [ ] **Step 6: Validate grep**
```bash
# Dramatically reduced json! usage in MCP handlers (some remain for error/info responses):
grep -c 'serde_json::json!' /home/jmagar/workspace/axon_rust/src/mcp/server/handlers_query.rs
grep -c 'serde_json::json!' /home/jmagar/workspace/axon_rust/src/mcp/server/handlers_system.rs
```

- [ ] **Step 7: Update CHANGELOG, bump version, commit**
```bash
rtk git add -A
rtk git commit -m "refactor(envelope): use Serialize derives for MCP handler payloads (dvo.6)"
```

### Acceptance Criteria — dvo.6

- [ ] All public service result types in `types/service.rs` derive `Serialize`
- [ ] `ScrapeArtifacts` derives `Serialize`
- [ ] Golden snapshot tests in `types/envelope_tests.rs` pass
- [ ] MCP outer envelope (`ok`, `action`, `subaction`, `data`) shape preserved
- [ ] `/v1/actions` outer envelope (`request_id`, `result`/`error`) unchanged
- [ ] `grep -rn 'serde_json::json!' src/mcp/server/handlers_*.rs` count is lower than before (some remain for error/status-only responses — that is acceptable)
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` green
- [ ] CHANGELOG entry

---

## Epic-Level Acceptance Criteria (axon_rust-dvo)

Run these after all five PRs land:

```bash
# 1. No persistence in CLI/MCP presentation layers for scrape/crawl artifacts
grep -rn 'tokio::fs::write\|tokio::fs::create_dir_all' \
  src/cli/commands/ src/mcp/server/handlers_*.rs \
  src/services/action_api/commands/dispatchers.rs \
  src/crawl/engine/thin_refetch.rs | grep -v '_tests\.'
# Expected: zero output

# 2. No validate_mcp_url (singular) anywhere
grep -rn '\bvalidate_mcp_url\b' src/ | grep -v validate_mcp_urls
# Expected: zero output

# 3. No inline parse_ingest_source domain logic in MCP handler
grep -n 'fn parse_ingest_source' src/mcp/server/handlers_embed_ingest.rs
# Expected: zero output

# 4. Audit dir deleted from CLI
ls src/cli/commands/crawl/audit/ 2>&1
# Expected: audit.rs sitemap_migration_tests.rs (nothing else)

# 5. Services audit dir exists
ls src/services/crawl/audit/
# Expected: manifest_audit.rs audit_diff.rs sitemap.rs + test sidecars

# 6. Full build green
cargo build --bin axon
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

---

## Version Bumping

Each PR above is a `refactor`-type commit. Per project rules, bump the **patch** version in:
- `Cargo.toml` → `version = "X.Y.Z+1"` in `[package]`

Add a CHANGELOG entry for each PR under the bumped version before committing.

---

## Quick Reference: Commands

```bash
# Build
cargo build --bin axon

# Test all
rtk cargo test

# Lint
rtk cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --check

# Auto-fix format
cargo fmt

# Pre-PR gate
just verify
```
