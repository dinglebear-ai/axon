# Port Webclaw `diff` and `brand` Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `axon diff <url-a> <url-b>` and `axon brand <url>` CLI commands — ports of webclaw's diff and brand tools — each with a matching MCP action, service layer function, typed result struct, CLI command handler, and sidecar tests.

**Architecture:** Both tools follow the established summarize pattern: service function in `src/services/` returns a typed result struct defined in `src/services/types/service.rs`, a CLI handler in `src/cli/commands/` formats output, and an MCP request struct plus `AxonRequest` variant in `src/mcp/schema/` enables tool calls. Diff is purely deterministic (fetch two URLs, compare markdown/metadata/links). Brand is a pure DOM/CSS analysis (no LLM, no network calls beyond the initial fetch). Neither command needs the job queue or `ServiceContext` — both take `&Config` only, like `run_summarize`.

**Tech Stack:** Rust, `similar` crate (unified diff), `scraper` crate (HTML DOM/CSS extraction), `once_cell` (lazy regex compilation), `regex`, `url`, `serde`/`serde_json`; axon's existing `services::scrape`, `http_client()`, `ConfigOverrides`, and `ScrapeFormat`.

**License boundary:** Webclaw is AGPL-3.0, axon is MIT. All logic below is a clean reimplementation of the *behavior* described by reading webclaw's code. No source from webclaw is copied.

---

## Scope Check

Two independent tools. Each produces working, testable software independently. They share a dependency bump (add `similar` and `scraper` to `Cargo.toml`). That shared step is Task 1; Tasks 2–5 implement `diff`; Tasks 6–9 implement `brand`.

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `Cargo.toml` | Modify | Add `similar = "2"`, `scraper = "0.22"`, `once_cell = "1"` |
| `src/services/types/service.rs` | Modify | Add `DiffResult`, `DiffStatus`, `MetadataChange`, `ContentDiff`, `BrandIdentity`, `BrandColor`, `ColorUsage`, `LogoVariant` |
| `src/services/diff.rs` | Create | `diff(cfg, url_a, url_b, tx)` service function |
| `src/services/diff_tests.rs` | Create | Sidecar tests for `diff.rs` (pure computation, no network) |
| `src/services/brand.rs` | Create | `brand(cfg, url, tx)` service function; all DOM/CSS extraction logic |
| `src/services/brand_tests.rs` | Create | Sidecar tests for `brand.rs` (pass raw HTML strings, no network) |
| `src/cli/commands/diff.rs` | Create | `run_diff(cfg)` CLI handler + output formatting |
| `src/cli/commands/diff_tests.rs` | Create | Sidecar tests for CLI output formatting |
| `src/cli/commands/brand.rs` | Create | `run_brand(cfg)` CLI handler + output formatting |
| `src/cli/commands/brand_tests.rs` | Create | Sidecar tests for CLI output formatting |
| `src/cli/commands.rs` | Modify | `pub mod diff; pub use diff::run_diff; pub mod brand; pub use brand::run_brand;` |
| `src/core/config/types/enums.rs` | Modify | Add `CommandKind::Diff` and `CommandKind::Brand` variants |
| `src/core/config/cli.rs` | Modify | Add `Diff(DiffArgs)` and `Brand(ScrapeArgs)` to `CliCommand` |
| `src/core/config/parse/build_config/command_dispatch.rs` | Modify | Map `CliCommand::Diff` and `CliCommand::Brand` → `CommandKind` |
| `src/lib.rs` | Modify | Import `run_diff`, `run_brand`; add `CommandKind::Diff` and `CommandKind::Brand` arms in `run_once()` |
| `src/mcp/schema/requests.rs` | Modify | Add `DiffRequest` struct |
| `src/mcp/schema/utility.rs` | Modify | Add `BrandRequest` struct |
| `src/mcp/schema.rs` | Modify | Add `Diff(DiffRequest)` and `Brand(BrandRequest)` to `AxonRequest` |
| `src/services/action_api.rs` | Modify | Add `Diff` and `Brand` arms to dispatch match and `required_scope` |
| `src/services/action_api/commands/dispatchers.rs` | Modify | Add `dispatch_diff` and `dispatch_brand` |
| `src/services/action_api/commands.rs` | Modify | Re-export `dispatch_diff`, `dispatch_brand` |
| `src/mcp/server/handlers_query.rs` | Modify | Add `handle_diff` and `handle_brand` handler functions |
| `src/mcp/server/handlers_system.rs` | Modify | Add `"diff"` and `"brand"` to the help action map |
| `docs/MCP-TOOL-SCHEMA.md` | Modify | Document `diff` and `brand` actions |
| `CLAUDE.md` | Modify | Add `diff` and `brand` to the command table |

---

## Task 1: Add Dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Open `Cargo.toml` and add the three new crates**

Find the block of direct dependencies (around line 120–145, after `regex = "1"`) and add:

```toml
similar = "2"
scraper = "0.22"
once_cell = "1"
```

They should sit alongside existing crates like `regex = "1"` and `url = "2"`. Do not add them to `[features]` — they are always-on dependencies.

- [ ] **Step 2: Verify compilation**

```bash
rtk cargo check --bin axon 2>&1 | head -30
```

Expected: no errors (new crates compile cleanly, nothing uses them yet).

- [ ] **Step 3: Commit**

```bash
rtk git add Cargo.toml Cargo.lock
rtk git commit -m "chore(deps): add similar, scraper, once_cell for diff+brand port"
```

---

## Task 2: Add `DiffResult` and `BrandIdentity` types to the service layer

**Files:**
- Modify: `src/services/types/service.rs`

- [ ] **Step 1: Locate the end of `service.rs`**

The file is large. Open it and find the last struct definition (near the `ResearchResult` around line 930). You will append new types after it.

- [ ] **Step 2: Add diff result types**

Append to `src/services/types/service.rs`:

```rust
// ── diff ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Same,
    Changed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetadataChange {
    pub field: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LinkEntry {
    pub href: String,
    pub text: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiffResult {
    pub url_a: String,
    pub url_b: String,
    pub status: DiffStatus,
    /// Unified diff of the markdown content, if any changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_diff: Option<String>,
    pub metadata_changes: Vec<MetadataChange>,
    pub links_added: Vec<LinkEntry>,
    pub links_removed: Vec<LinkEntry>,
    pub word_count_delta: i64,
}
```

- [ ] **Step 3: Add brand result types**

Still in `src/services/types/service.rs`, append after the diff block:

```rust
// ── brand ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorUsage {
    Primary,
    Secondary,
    Background,
    Text,
    Accent,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrandColor {
    pub hex: String,
    pub usage: ColorUsage,
    pub count: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogoVariant {
    pub url: String,
    /// "favicon" | "apple-touch-icon" | "logo" | "og-image" | "svg"
    pub kind: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrandResult {
    pub url: String,
    pub name: Option<String>,
    pub colors: Vec<BrandColor>,
    pub fonts: Vec<String>,
    pub logos: Vec<LogoVariant>,
    pub logo_url: Option<String>,
    pub favicon_url: Option<String>,
    pub og_image: Option<String>,
}
```

- [ ] **Step 4: Run cargo check**

```bash
rtk cargo check --bin axon 2>&1 | head -30
```

Expected: no errors. The types are defined but nothing uses them yet.

- [ ] **Step 5: Commit**

```bash
rtk git add src/services/types/service.rs
rtk git commit -m "feat(types): add DiffResult and BrandResult service types"
```

---

## Task 3: Implement `src/services/diff.rs`

**Files:**
- Create: `src/services/diff.rs`
- Create: `src/services/diff_tests.rs`
- Modify: `src/services/mod.rs` (or wherever `services` re-exports are declared — see note below)

> **Note on module wiring for services:** `src/services.rs` is the crate root that declares sub-modules with `pub mod`. Find it at `src/services.rs` (file-per-module style, not `mod.rs`). Add `pub mod diff;` there alongside the existing service modules.

- [ ] **Step 1: Write the sidecar test file first (red phase)**

Create `src/services/diff_tests.rs`:

```rust
use super::*;
use crate::services::types::{DiffResult, DiffStatus};

/// Helper: build a minimal pair of markdown strings and call the pure diff logic.
fn run_pure_diff(md_a: &str, md_b: &str) -> DiffResult {
    compute_diff(
        "https://example.com/a",
        md_a,
        &[],
        &std::collections::HashMap::new(),
        "https://example.com/b",
        md_b,
        &[],
        &std::collections::HashMap::new(),
    )
}

#[test]
fn test_identical_content_is_same() {
    let r = run_pure_diff("# Hello\n\nContent.", "# Hello\n\nContent.");
    assert_eq!(r.status, DiffStatus::Same);
    assert!(r.text_diff.is_none());
    assert!(r.metadata_changes.is_empty());
    assert_eq!(r.word_count_delta, 0);
}

#[test]
fn test_changed_content_produces_diff() {
    let r = run_pure_diff("# Hello\n\nOld paragraph.", "# Hello\n\nNew paragraph.");
    assert_eq!(r.status, DiffStatus::Changed);
    let diff_text = r.text_diff.unwrap();
    assert!(diff_text.contains('-'), "should have removal markers");
    assert!(diff_text.contains('+'), "should have addition markers");
}

#[test]
fn test_word_count_delta_positive() {
    let r = run_pure_diff("one two three", "one two three four five");
    assert_eq!(r.word_count_delta, 2);
}

#[test]
fn test_word_count_delta_negative() {
    let r = run_pure_diff("one two three four five", "one two three");
    assert_eq!(r.word_count_delta, -2);
}

#[test]
fn test_link_added() {
    use crate::services::types::LinkEntry;
    let links_b = vec![LinkEntry {
        href: "https://new.com".to_string(),
        text: "New".to_string(),
    }];
    let mut result = compute_diff(
        "https://example.com/a",
        "Content",
        &[],
        &Default::default(),
        "https://example.com/b",
        "Content",
        &links_b,
        &Default::default(),
    );
    assert_eq!(result.links_added.len(), 1);
    assert_eq!(result.links_added[0].href, "https://new.com");
    assert!(result.links_removed.is_empty());
}

#[test]
fn test_link_removed() {
    use crate::services::types::LinkEntry;
    let links_a = vec![LinkEntry {
        href: "https://old.com".to_string(),
        text: "Old".to_string(),
    }];
    let result = compute_diff(
        "https://example.com/a",
        "Content",
        &links_a,
        &Default::default(),
        "https://example.com/b",
        "Content",
        &[],
        &Default::default(),
    );
    assert!(result.links_added.is_empty());
    assert_eq!(result.links_removed.len(), 1);
    assert_eq!(result.links_removed[0].href, "https://old.com");
}

#[test]
fn test_metadata_title_change() {
    let mut meta_a = std::collections::HashMap::new();
    meta_a.insert("title".to_string(), serde_json::Value::String("Old Title".to_string()));
    let mut meta_b = std::collections::HashMap::new();
    meta_b.insert("title".to_string(), serde_json::Value::String("New Title".to_string()));

    let result = compute_diff(
        "https://example.com/a",
        "Content",
        &[],
        &meta_a,
        "https://example.com/b",
        "Content",
        &[],
        &meta_b,
    );
    assert_eq!(result.status, DiffStatus::Changed);
    assert_eq!(result.metadata_changes.len(), 1);
    assert_eq!(result.metadata_changes[0].field, "title");
    assert_eq!(result.metadata_changes[0].old.as_deref(), Some("Old Title"));
    assert_eq!(result.metadata_changes[0].new.as_deref(), Some("New Title"));
}
```

- [ ] **Step 2: Run the test to confirm it fails (red)**

```bash
rtk cargo test diff_tests 2>&1 | tail -20
```

Expected: compile error — `compute_diff` not found.

- [ ] **Step 3: Implement `src/services/diff.rs`**

Create `src/services/diff.rs`:

```rust
//! Diff service: fetch two URLs and compare their content.
//!
//! The pure computation (`compute_diff`) is separated from I/O (`diff`) so it
//! can be tested without network calls.

use std::collections::{HashMap, HashSet};
use std::error::Error;

use similar::TextDiff;

use crate::core::config::{Config, ConfigOverrides, ScrapeFormat};
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::scrape;
use crate::services::types::{DiffResult, DiffStatus, LinkEntry, MetadataChange};
use tokio::sync::mpsc;

/// Fetch `url_a` and `url_b`, then compute and return a `DiffResult`.
pub async fn diff(
    cfg: &Config,
    url_a: &str,
    url_b: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<DiffResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("diff: fetching {url_a} and {url_b}"),
        },
    )
    .await;

    let scrape_cfg = cfg.apply_overrides(&ConfigOverrides {
        format: Some(ScrapeFormat::Markdown),
        output_path: Some(None),
        ..ConfigOverrides::default()
    });

    let results = scrape::scrape_batch(&scrape_cfg, &[url_a.to_string(), url_b.to_string()], tx.clone()).await?;

    let (doc_a, doc_b) = match results.as_slice() {
        [a, b] => (a, b),
        _ => return Err("diff requires exactly two URLs to be fetched successfully".into()),
    };

    let links_a = extract_links_from_payload(&doc_a.payload);
    let links_b = extract_links_from_payload(&doc_b.payload);

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "diff: computing changes".to_string(),
        },
    )
    .await;

    Ok(compute_diff(
        &doc_a.url,
        &doc_a.markdown,
        &links_a,
        &doc_a.payload,
        &doc_b.url,
        &doc_b.markdown,
        &links_b,
        &doc_b.payload,
    ))
}

/// Pure diff computation — no I/O.
///
/// Exposed as `pub(crate)` so sidecar tests can call it directly without
/// requiring network access.
pub(crate) fn compute_diff(
    url_a: &str,
    markdown_a: &str,
    links_a: &[LinkEntry],
    meta_a: &HashMap<String, serde_json::Value>,
    url_b: &str,
    markdown_b: &str,
    links_b: &[LinkEntry],
    meta_b: &HashMap<String, serde_json::Value>,
) -> DiffResult {
    let text_diff = compute_text_diff(markdown_a, markdown_b);
    let metadata_changes = compute_metadata_changes(meta_a, meta_b);
    let (links_added, links_removed) = compute_link_changes(links_a, links_b);
    let word_count_a = markdown_a.split_whitespace().count() as i64;
    let word_count_b = markdown_b.split_whitespace().count() as i64;
    let word_count_delta = word_count_b - word_count_a;

    let status = if text_diff.is_none() && metadata_changes.is_empty() {
        DiffStatus::Same
    } else {
        DiffStatus::Changed
    };

    DiffResult {
        url_a: url_a.to_string(),
        url_b: url_b.to_string(),
        status,
        text_diff,
        metadata_changes,
        links_added,
        links_removed,
        word_count_delta,
    }
}

fn compute_text_diff(old: &str, new: &str) -> Option<String> {
    if old == new {
        return None;
    }
    let d = TextDiff::from_lines(old, new);
    let unified = d.unified_diff().context_radius(3).header("a", "b").to_string();
    if unified.is_empty() { None } else { Some(unified) }
}

const COMPARED_META_FIELDS: &[&str] = &[
    "title", "description", "author", "published_date", "language",
    "url", "site_name", "image", "favicon",
];

fn compute_metadata_changes(
    meta_a: &HashMap<String, serde_json::Value>,
    meta_b: &HashMap<String, serde_json::Value>,
) -> Vec<MetadataChange> {
    let mut changes = Vec::new();
    for &field in COMPARED_META_FIELDS {
        let old = meta_a.get(field).and_then(|v| v.as_str()).map(str::to_string);
        let new = meta_b.get(field).and_then(|v| v.as_str()).map(str::to_string);
        if old != new {
            changes.push(MetadataChange { field: field.to_string(), old, new });
        }
    }
    changes
}

fn compute_link_changes(
    links_a: &[LinkEntry],
    links_b: &[LinkEntry],
) -> (Vec<LinkEntry>, Vec<LinkEntry>) {
    let hrefs_a: HashSet<&str> = links_a.iter().map(|l| l.href.as_str()).collect();
    let hrefs_b: HashSet<&str> = links_b.iter().map(|l| l.href.as_str()).collect();

    let added = links_b
        .iter()
        .filter(|l| !hrefs_a.contains(l.href.as_str()))
        .cloned()
        .collect();
    let removed = links_a
        .iter()
        .filter(|l| !hrefs_b.contains(l.href.as_str()))
        .cloned()
        .collect();
    (added, removed)
}

/// Extract links from a scrape payload's `links` field if present.
fn extract_links_from_payload(payload: &HashMap<String, serde_json::Value>) -> Vec<LinkEntry> {
    payload
        .get("links")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let href = item.get("href")?.as_str()?.to_string();
                    let text = item
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(LinkEntry { href, text })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run the tests (green phase)**

```bash
rtk cargo test diff_tests 2>&1 | tail -20
```

Expected: all 7 tests pass. Fix any type mismatches and re-run.

- [ ] **Step 5: Register the module in `src/services.rs`**

Open `src/services.rs` and add `pub mod diff;` alongside the other service modules (alphabetically near `debug`):

```rust
pub mod diff;
```

- [ ] **Step 6: Run cargo check**

```bash
rtk cargo check --bin axon 2>&1 | head -30
```

Expected: clean.

- [ ] **Step 7: Commit**

```bash
rtk git add src/services/diff.rs src/services/diff_tests.rs src/services.rs
rtk git commit -m "feat(services): implement diff service with pure compute_diff and sidecar tests"
```

---

## Task 4: Implement `src/cli/commands/diff.rs` CLI handler

**Files:**
- Create: `src/cli/commands/diff.rs`
- Create: `src/cli/commands/diff_tests.rs`

- [ ] **Step 1: Write the sidecar test file first (red phase)**

Create `src/cli/commands/diff_tests.rs`:

```rust
use super::*;
use crate::services::types::{DiffResult, DiffStatus, LinkEntry, MetadataChange};

fn make_same_result() -> DiffResult {
    DiffResult {
        url_a: "https://example.com/a".to_string(),
        url_b: "https://example.com/b".to_string(),
        status: DiffStatus::Same,
        text_diff: None,
        metadata_changes: vec![],
        links_added: vec![],
        links_removed: vec![],
        word_count_delta: 0,
    }
}

fn make_changed_result() -> DiffResult {
    DiffResult {
        url_a: "https://example.com/a".to_string(),
        url_b: "https://example.com/b".to_string(),
        status: DiffStatus::Changed,
        text_diff: Some("--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n".to_string()),
        metadata_changes: vec![MetadataChange {
            field: "title".to_string(),
            old: Some("Old".to_string()),
            new: Some("New".to_string()),
        }],
        links_added: vec![LinkEntry {
            href: "https://new.com".to_string(),
            text: "New Link".to_string(),
        }],
        links_removed: vec![],
        word_count_delta: 1,
    }
}

#[test]
fn test_format_same_result_human() {
    // Verify format_diff_result returns without error for a Same result
    let mut buf = Vec::new();
    let result = make_same_result();
    // We test the pure formatting helper, not println! directly
    let output = format_diff_summary(&result);
    assert!(output.contains("same") || output.contains("Same") || output.contains("no changes"),
        "same result should indicate no changes, got: {output}");
}

#[test]
fn test_format_changed_result_human() {
    let result = make_changed_result();
    let output = format_diff_summary(&result);
    assert!(output.contains("changed") || output.contains("Changed"),
        "changed result should indicate changes, got: {output}");
}

#[test]
fn test_format_diff_shows_word_count_delta() {
    let result = make_changed_result();
    let output = format_diff_summary(&result);
    assert!(output.contains('+') || output.contains("word"),
        "output should mention word count delta, got: {output}");
}
```

- [ ] **Step 2: Run the test to confirm it fails (red)**

```bash
rtk cargo test diff_tests -- --test-output immediate 2>&1 | tail -20
```

Expected: compile error — `format_diff_summary` not found.

- [ ] **Step 3: Implement `src/cli/commands/diff.rs`**

Create `src/cli/commands/diff.rs`:

```rust
use crate::core::config::Config;
use crate::core::logging::{log_done, log_info};
use crate::core::ui::{muted, primary, print_option, print_phase};
use crate::services::diff as diff_svc;
use crate::services::types::{DiffResult, DiffStatus};
use std::error::Error;

pub async fn run_diff(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let (url_a, url_b) = parse_diff_urls(cfg)?;

    log_info(&format!("command=diff url_a={url_a} url_b={url_b}"));
    let result = diff_svc::diff(cfg, &url_a, &url_b, None).await?;

    emit_diff_result(cfg, &result)?;

    log_done(&format!(
        "command=diff status={:?} metadata_changes={} links_added={} links_removed={}",
        result.status,
        result.metadata_changes.len(),
        result.links_added.len(),
        result.links_removed.len(),
    ));
    Ok(())
}

fn parse_diff_urls(cfg: &Config) -> Result<(String, String), Box<dyn Error>> {
    match cfg.positional.as_slice() {
        [a, b, ..] => Ok((a.clone(), b.clone())),
        [_] => Err("diff requires two URLs: axon diff <url-a> <url-b>".into()),
        [] => Err("diff requires two URLs: axon diff <url-a> <url-b>".into()),
    }
}

pub(crate) fn emit_diff_result(cfg: &Config, result: &DiffResult) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(());
    }

    let status_label = match result.status {
        DiffStatus::Same => "no changes",
        DiffStatus::Changed => "changed",
    };
    print_phase("◑", "Diff", &format!("{} vs {}", result.url_a, result.url_b));
    print_option("status", status_label);
    print_option("wordCountDelta", &format!("{:+}", result.word_count_delta));
    print_option("metadataChanges", &result.metadata_changes.len().to_string());
    print_option("linksAdded", &result.links_added.len().to_string());
    print_option("linksRemoved", &result.links_removed.len().to_string());

    if !result.metadata_changes.is_empty() {
        println!("\n{}", primary("Metadata Changes"));
        for change in &result.metadata_changes {
            let old = change.old.as_deref().unwrap_or("(none)");
            let new = change.new.as_deref().unwrap_or("(none)");
            println!("  {} {}: {} → {}", muted("~"), change.field, old, new);
        }
    }

    if let Some(ref diff_text) = result.text_diff {
        println!("\n{}", primary("Content Diff"));
        println!("{}", diff_text);
    }

    if !result.links_added.is_empty() {
        println!("\n{}", primary("Links Added"));
        for link in &result.links_added {
            println!("  {} {} ({})", muted("+"), link.href, link.text);
        }
    }

    if !result.links_removed.is_empty() {
        println!("\n{}", primary("Links Removed"));
        for link in &result.links_removed {
            println!("  {} {} ({})", muted("-"), link.href, link.text);
        }
    }

    Ok(())
}

/// Pure formatting helper exposed for testing.
pub(crate) fn format_diff_summary(result: &DiffResult) -> String {
    match result.status {
        DiffStatus::Same => format!(
            "same (no changes) word_count_delta={:+}",
            result.word_count_delta
        ),
        DiffStatus::Changed => format!(
            "changed word_count_delta={:+} metadata={} links_added={} links_removed={}",
            result.word_count_delta,
            result.metadata_changes.len(),
            result.links_added.len(),
            result.links_removed.len(),
        ),
    }
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run the tests (green phase)**

```bash
rtk cargo test diff_tests 2>&1 | tail -20
```

Expected: 3 tests pass. Fix any issues and re-run.

- [ ] **Step 5: Commit**

```bash
rtk git add src/cli/commands/diff.rs src/cli/commands/diff_tests.rs
rtk git commit -m "feat(cli): add diff command handler with human+JSON output formatting"
```

---

## Task 5: Wire `diff` into CLI, `CommandKind`, `lib.rs`, and MCP

**Files:**
- Modify: `src/cli/commands.rs`
- Modify: `src/core/config/types/enums.rs`
- Modify: `src/core/config/cli.rs`
- Modify: `src/core/config/parse/build_config/command_dispatch.rs`
- Modify: `src/lib.rs`
- Modify: `src/mcp/schema/requests.rs`
- Modify: `src/mcp/schema.rs`
- Modify: `src/services/action_api.rs`
- Modify: `src/services/action_api/commands/dispatchers.rs`
- Modify: `src/services/action_api/commands.rs`
- Modify: `src/mcp/server/handlers_query.rs`
- Modify: `src/mcp/server/handlers_system.rs`

- [ ] **Step 1: Add `run_diff` to `src/cli/commands.rs`**

Open `src/cli/commands.rs`. Add:

```rust
pub mod diff;
pub use diff::run_diff;
```

Place it alphabetically (between `debug` and `doctor` or `domains`).

- [ ] **Step 2: Add `CommandKind::Diff` to `src/core/config/types/enums.rs`**

In the `CommandKind` enum, add `Diff,` (alphabetically near `Doctor` and `Dedupe`). In the `as_str()` match, add:

```rust
Self::Diff => "diff",
```

- [ ] **Step 3: Add `Diff(DiffArgs)` to `src/core/config/cli.rs`**

In `CliCommand`, add near `Doctor`:

```rust
/// Diff two URLs — show what changed between them
Diff(DiffArgs),
```

Then define `DiffArgs` at the bottom of the file with the other structs:

```rust
#[derive(Debug, Args)]
pub(super) struct DiffArgs {
    /// First URL (baseline)
    #[arg(value_name = "URL_A")]
    pub(super) url_a: String,
    /// Second URL (comparison)
    #[arg(value_name = "URL_B")]
    pub(super) url_b: String,
}
```

Also add `DiffArgs` to the `use super::cli::{..., DiffArgs, ...}` import in `parse/build_config/command_dispatch.rs` (you'll do that in the next step).

- [ ] **Step 4: Map `CliCommand::Diff` in `command_dispatch.rs`**

Open `src/core/config/parse/build_config/command_dispatch.rs`. Add the import for `DiffArgs` to the existing `use super::super::cli::{...}` block.

Then add the dispatch arm alphabetically among the other simple commands:

```rust
CliCommand::Diff(args) => {
    out.command = CommandKind::Diff;
    out.positional = vec![args.url_a, args.url_b];
}
```

- [ ] **Step 5: Add the dispatch arm to `src/lib.rs`**

In `src/lib.rs`, add `run_diff` to the import line:

```rust
use self::cli::commands::{
    ..., run_diff, ...
};
```

In `run_once()`, add:

```rust
CommandKind::Diff => run_diff(cfg).await?,
```

Place it alphabetically (near `CommandKind::Doctor` / `CommandKind::Debug`).

- [ ] **Step 6: Add `DiffRequest` to `src/mcp/schema/requests.rs`**

Append at the bottom of `requests.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiffRequest {
    /// First (baseline) URL
    pub url_a: Option<String>,
    /// Second (comparison) URL
    pub url_b: Option<String>,
    /// Rendering engine override (http | chrome | auto_switch)
    pub render_mode: Option<McpRenderMode>,
    pub response_mode: Option<ResponseMode>,
}
```

- [ ] **Step 7: Add `Diff(DiffRequest)` to `src/mcp/schema.rs`**

In the `AxonRequest` enum, add `Diff(DiffRequest),` (near `Dedupe`). Make sure `DiffRequest` is included in the `pub use requests::*;` re-export — since it is now in `requests.rs`, this is automatic.

- [ ] **Step 8: Add diff to `src/services/action_api.rs`**

Find the large `match req` dispatch block and add:

```rust
AxonRequest::Diff(req) => commands::dispatch_diff(service_context, req).await,
```

Find the `required_scope` match and add (diff reads two URLs, counts as read):

```rust
AxonRequest::Diff(_) => Some("axon:read"),
```

Find the `action_name` match and add:

```rust
AxonRequest::Diff(_) => "diff",
```

- [ ] **Step 9: Add `dispatch_diff` to `src/services/action_api/commands/dispatchers.rs`**

Import at top of `dispatchers.rs`:

```rust
use crate::services::diff as diff_svc;
use crate::mcp::schema::DiffRequest;
```

Append the function:

```rust
pub async fn dispatch_diff(
    service_context: &ServiceContext,
    req: DiffRequest,
) -> Result<serde_json::Value, ClientActionError> {
    let url_a = req.url_a.ok_or_else(|| {
        ClientActionError::new("invalid_request", "url_a is required for diff", false, None)
    })?;
    let url_b = req.url_b.ok_or_else(|| {
        ClientActionError::new("invalid_request", "url_b is required for diff", false, None)
    })?;

    let cfg_overrides = ConfigOverrides {
        render_mode: req.render_mode.map(map_render_mode),
        ..ConfigOverrides::default()
    };
    let cfg = service_context.cfg.apply_overrides(&cfg_overrides);

    let result = diff_svc::diff(&cfg, &url_a, &url_b, None)
        .await
        .map_err(internal_error)?;

    serde_json::to_value(result)
        .map_err(|e| ClientActionError::new("internal_error", format!("serialize diff result: {e}"), false, None))
}
```

- [ ] **Step 10: Re-export from `src/services/action_api/commands.rs`**

Add `dispatch_diff` to the `pub(super) use dispatchers::{...}` line.

- [ ] **Step 11: Add `handle_diff` to `src/mcp/server/handlers_query.rs`**

Add import: `use crate::mcp::schema::DiffRequest;`

Add handler function (use the pattern from `handle_summarize`):

```rust
pub(super) async fn handle_diff(
    service_context: &ServiceContext,
    req: DiffRequest,
) -> Result<AxonToolResponse, ErrorData> {
    let url_a = req
        .url_a
        .clone()
        .ok_or_else(|| invalid_params("url_a is required for diff"))?;
    let url_b = req
        .url_b
        .clone()
        .ok_or_else(|| invalid_params("url_b is required for diff"))?;

    let cfg = service_context.cfg.as_ref();
    let result = diff_svc::diff(cfg, &url_a, &url_b, None)
        .await
        .map_err(|e| logged_internal_error("diff", e.as_ref()))?;

    let data = serde_json::to_value(&result)
        .map_err(|e| internal_error(format!("serialize diff result: {e}")))?;

    Ok(AxonToolResponse::ok("diff", "diff", data))
}
```

Also add `use crate::services::diff as diff_svc;` to the imports at the top of the file.

Wire the handler into the dispatch in `src/mcp/server.rs` — find the `AxonRequest::Summarize` arm and add nearby:

```rust
AxonRequest::Diff(req) => {
    handlers_query::handle_diff(&self.service_context, req).await
}
```

- [ ] **Step 12: Update help action in `src/mcp/server/handlers_system.rs`**

Find the JSON object that lists available actions (look for `"summarize"`) and add `"diff"` to the same list.

- [ ] **Step 13: Verify compilation**

```bash
rtk cargo check --bin axon 2>&1 | head -40
```

Expected: no errors. Fix any missing imports or enum variant exhaustion.

- [ ] **Step 14: Run all diff tests**

```bash
rtk cargo test diff 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 15: Commit**

```bash
rtk git add \
  src/cli/commands.rs \
  src/core/config/types/enums.rs \
  src/core/config/cli.rs \
  src/core/config/parse/build_config/command_dispatch.rs \
  src/lib.rs \
  src/mcp/schema/requests.rs \
  src/mcp/schema.rs \
  src/services/action_api.rs \
  src/services/action_api/commands/dispatchers.rs \
  src/services/action_api/commands.rs \
  src/mcp/server/handlers_query.rs \
  src/mcp/server/handlers_system.rs
rtk git commit -m "feat(diff): wire diff command through CLI, CommandKind, lib.rs, and MCP"
```

---

## Task 6: Implement `src/services/brand.rs` (DOM/CSS extraction)

**Files:**
- Create: `src/services/brand.rs`
- Create: `src/services/brand_tests.rs`
- Modify: `src/services.rs` (add `pub mod brand;`)

This is the largest task. The logic is entirely pure-Rust DOM/CSS analysis using `scraper` and `regex` crate. No LLM, no network call inside `extract_brand_from_html`.

- [ ] **Step 1: Write the sidecar test file first (red phase)**

Create `src/services/brand_tests.rs`:

```rust
use super::*;

#[test]
fn test_extracts_hex_colors() {
    let html = r#"<html><head><style>
        .header { background-color: #3498db; }
        .text { color: #2c3e50; }
    </style></head><body></body></html>"#;

    let brand = extract_brand_from_html(html, None);
    let hexes: Vec<&str> = brand.colors.iter().map(|c| c.hex.as_str()).collect();
    assert!(hexes.contains(&"#3498DB"), "should find header bg color");
    assert!(hexes.contains(&"#2C3E50"), "should find text color");
}

#[test]
fn test_filters_boring_colors() {
    let html = r#"<html><head><style>
        body { background-color: #ffffff; color: #000000; }
        .brand { color: #3498db; }
    </style></head><body></body></html>"#;

    let brand = extract_brand_from_html(html, None);
    let hexes: Vec<&str> = brand.colors.iter().map(|c| c.hex.as_str()).collect();
    assert!(!hexes.contains(&"#FFFFFF"), "white should be filtered");
    assert!(!hexes.contains(&"#000000"), "black should be filtered");
    assert!(hexes.contains(&"#3498DB"), "brand color should survive");
}

#[test]
fn test_extracts_fonts() {
    let html = r#"<html><head><style>
        body { font-family: "Inter", "Helvetica Neue", sans-serif; }
        code { font-family: 'Fira Code', monospace; }
    </style></head><body></body></html>"#;

    let brand = extract_brand_from_html(html, None);
    assert!(brand.fonts.contains(&"Inter".to_string()), "should find Inter");
    assert!(brand.fonts.contains(&"Fira Code".to_string()), "should find Fira Code");
    assert!(!brand.fonts.contains(&"sans-serif".to_string()), "generic should be excluded");
    assert!(!brand.fonts.contains(&"monospace".to_string()), "generic should be excluded");
}

#[test]
fn test_extracts_favicon() {
    let html = r#"<html><head>
        <link rel="icon" href="/favicon.ico">
    </head><body></body></html>"#;

    let brand = extract_brand_from_html(html, Some("https://example.com"));
    assert_eq!(brand.favicon_url.as_deref(), Some("https://example.com/favicon.ico"));
}

#[test]
fn test_extracts_logo_by_class() {
    let html = r#"<html><body>
        <header>
            <img class="site-logo" src="/logo.svg" alt="Brand">
        </header>
    </body></html>"#;

    let brand = extract_brand_from_html(html, Some("https://example.com"));
    assert_eq!(brand.logo_url.as_deref(), Some("https://example.com/logo.svg"));
}

#[test]
fn test_extracts_brand_name_from_og_site_name() {
    let html = r#"<html><head>
        <meta property="og:site_name" content="Acme Corp">
    </head><body></body></html>"#;

    let brand = extract_brand_from_html(html, None);
    assert_eq!(brand.name.as_deref(), Some("Acme Corp"));
}

#[test]
fn test_css_custom_properties() {
    let html = r#"<html><head><style>
        :root {
            --primary: #3b82f6;
            --spacing: 1rem;
        }
    </style></head><body></body></html>"#;

    let brand = extract_brand_from_html(html, None);
    let hexes: Vec<&str> = brand.colors.iter().map(|c| c.hex.as_str()).collect();
    assert!(hexes.contains(&"#3B82F6"), "should find --primary CSS variable");
}

#[test]
fn test_empty_html_returns_empty_result() {
    let brand = extract_brand_from_html("", None);
    assert!(brand.colors.is_empty());
    assert!(brand.fonts.is_empty());
    assert!(brand.logo_url.is_none());
    assert!(brand.favicon_url.is_none());
}

#[test]
fn test_max_10_colors() {
    let colors: Vec<String> = (0..15u8)
        .map(|i| format!(".c{i} {{ color: #{:02X}{:02X}{:02X}; }}", 10 + i * 15, 20 + i * 10, 30 + i * 5))
        .collect();
    let html = format!("<html><head><style>{}</style></head><body></body></html>", colors.join("\n"));
    let brand = extract_brand_from_html(&html, None);
    assert!(brand.colors.len() <= 10, "should cap at 10 colors");
}

#[test]
fn test_rgb_color_parsing() {
    let html = r#"<html><head><style>
        .btn { background-color: rgb(52, 152, 219); }
    </style></head><body></body></html>"#;

    let brand = extract_brand_from_html(html, None);
    let hexes: Vec<&str> = brand.colors.iter().map(|c| c.hex.as_str()).collect();
    assert!(hexes.contains(&"#3498DB"), "rgb(52,152,219) -> #3498DB");
}
```

- [ ] **Step 2: Verify tests fail (red)**

```bash
rtk cargo test brand_tests 2>&1 | tail -10
```

Expected: compile error — `extract_brand_from_html` not found.

- [ ] **Step 3: Implement `src/services/brand.rs`**

This file will be close to 500 lines. Structure it as two sections: pure extraction logic (WASM-safe) and the async `brand()` entry point that fetches HTML and calls the pure logic.

Create `src/services/brand.rs`:

```rust
//! Brand identity extraction from a URL.
//!
//! The pure computation (`extract_brand_from_html`) takes raw HTML and performs
//! no network calls, making it fully testable without a running server.

use std::collections::HashMap;
use std::error::Error;

use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{Html, Selector};
use tokio::sync::mpsc;
use url::Url;

use crate::core::config::Config;
use crate::core::http::client::http_client;
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::types::{BrandColor, BrandResult, ColorUsage, LogoVariant};

// ── Regex patterns (compiled once) ──────────────────────────────────────────

static CSS_DECL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)([\w-]+)\s*:\s*([^;}{]+)").unwrap());
static CSS_VAR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)--([\w-]+)\s*:\s*([^;}{]+)").unwrap());
static HEX_COLOR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"#([0-9a-fA-F]{3})\b|#([0-9a-fA-F]{6})\b").unwrap());
static RGB_COLOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)rgb\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*\)").unwrap()
});
static RGBA_COLOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)rgba\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*[\d.]+\s*\)")
        .unwrap()
});
static HSL_COLOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)hsla?\(\s*(\d{1,3})\s*,\s*(\d{1,3})%\s*,\s*(\d{1,3})%\s*(?:,\s*[\d.]+\s*)?\)",
    )
    .unwrap()
});
static TW_COLOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:bg|text|border|ring|outline|shadow|accent|fill|stroke)-\[([^\]]+)\]",
    )
    .unwrap()
});
static FONT_SHORTHAND_FAMILY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?ix)(?:^|\s)(?:xx-small|x-small|small|medium|large|x-large|xx-large|larger|smaller|\d*\.?\d+(?:px|rem|em|pt|pc|in|cm|mm|%|vw|vh|vmin|vmax))(?:\s*/\s*[^\s,]+)?\s+(.+)$"#,
    )
    .unwrap()
});

macro_rules! sel {
    ($s:expr) => {{
        static S: Lazy<Selector> = Lazy::new(|| Selector::parse($s).unwrap());
        &*S
    }};
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Fetch `url` and extract brand identity.
pub async fn brand(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<BrandResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("brand: fetching {url}"),
        },
    )
    .await;

    let client = http_client()?;
    let mut req = client.get(url);
    for (k, v) in &cfg.custom_headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let response = req.send().await?;
    let html = response.text().await?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "brand: analyzing".to_string(),
        },
    )
    .await;

    let mut result = extract_brand_from_html(&html, Some(url));
    result.url = url.to_string();
    Ok(result)
}

// ── Pure extraction (no I/O) ─────────────────────────────────────────────────

/// Extract brand identity from raw HTML.
/// `page_url` is used only for resolving relative paths.
pub(crate) fn extract_brand_from_html(html: &str, page_url: Option<&str>) -> BrandResult {
    let doc = Html::parse_document(html);
    let base_url = page_url.and_then(|u| Url::parse(u).ok());

    let name = extract_brand_name(&doc);
    let css_sources = collect_css(&doc);
    let colors = extract_colors(&css_sources, name.as_deref());
    let fonts = extract_fonts(&css_sources, name.as_deref());
    let logo_url = find_logo(&doc, base_url.as_ref());
    let favicon_url = find_favicon(&doc, base_url.as_ref());
    let logos = find_all_logos(&doc, base_url.as_ref());
    let og_image = find_og_image(&doc, base_url.as_ref());

    BrandResult {
        url: page_url.unwrap_or("").to_string(),
        name,
        colors,
        fonts,
        logos,
        logo_url,
        favicon_url,
        og_image,
    }
}

// ── CSS collection ───────────────────────────────────────────────────────────

struct CssDecl {
    property: String,
    value: String,
}

fn collect_css(doc: &Html) -> Vec<CssDecl> {
    let mut decls = Vec::new();

    for el in doc.select(sel!("style")) {
        let text: String = el.text().collect();
        parse_declarations(&text, &mut decls);
        parse_css_variables(&text, &mut decls);
    }

    for el in doc.select(sel!("[style]")) {
        if let Some(style) = el.value().attr("style") {
            parse_declarations(style, &mut decls);
        }
    }

    for el in doc.select(sel!("[class]")) {
        if let Some(class) = el.value().attr("class") {
            parse_tailwind_colors(class, &mut decls);
        }
    }

    for el in doc.select(sel!("meta[name='theme-color']")) {
        if let Some(content) = el.value().attr("content") {
            decls.push(CssDecl { property: "background-color".to_string(), value: content.to_string() });
        }
    }

    for el in doc.select(sel!("link[rel='preload'][as='font']")) {
        if let Some(href) = el.value().attr("href") {
            if let Some(name) = extract_font_name_from_url(href) {
                decls.push(CssDecl { property: "font-family".to_string(), value: format!("\"{name}\"") });
            }
        }
    }

    for el in doc.select(sel!("link[rel='stylesheet']")) {
        if let Some(href) = el.value().attr("href") {
            if href.contains("fonts.googleapis.com") || href.contains("fonts.bunny.net") {
                for font in extract_google_fonts_from_url(href) {
                    decls.push(CssDecl { property: "font-family".to_string(), value: format!("\"{font}\"") });
                }
            }
        }
    }

    decls
}

fn parse_declarations(css_text: &str, out: &mut Vec<CssDecl>) {
    for cap in CSS_DECL.captures_iter(css_text) {
        let property = cap[1].to_ascii_lowercase();
        let value = cap[2].trim().to_string();
        out.push(CssDecl { property, value });
    }
}

fn parse_css_variables(css_text: &str, out: &mut Vec<CssDecl>) {
    for cap in CSS_VAR.captures_iter(css_text) {
        let var_name = cap[1].to_ascii_lowercase();
        let value = cap[2].trim().to_string();
        if is_color_value(&value) {
            let property = if var_name.contains("background") || var_name.contains("bg") {
                "background-color"
            } else if var_name.contains("text") || var_name.contains("foreground") || var_name.contains("fg") {
                "color"
            } else if var_name.contains("border") || var_name.contains("accent") {
                "border-color"
            } else {
                "color"
            };
            out.push(CssDecl { property: property.to_string(), value });
        }
    }
}

fn is_color_value(v: &str) -> bool {
    HEX_COLOR.is_match(v) || RGB_COLOR.is_match(v) || RGBA_COLOR.is_match(v) || HSL_COLOR.is_match(v)
}

fn parse_tailwind_colors(class: &str, out: &mut Vec<CssDecl>) {
    for cap in TW_COLOR.captures_iter(class) {
        let value = &cap[1];
        if is_color_value(value) {
            let full = cap.get(0).unwrap().as_str();
            let property = if full.starts_with("bg-") {
                "background-color"
            } else if full.starts_with("text-") {
                "color"
            } else if full.starts_with("border-") {
                "border-color"
            } else {
                "color"
            };
            out.push(CssDecl { property: property.to_string(), value: value.to_string() });
        }
    }
}

// ── Color extraction ─────────────────────────────────────────────────────────

const BORING_COLORS: &[&str] = &[
    "#FFFFFF", "#000000", "#F8F8F8", "#F5F5F5", "#EEEEEE", "#E5E5E5", "#DDDDDD",
    "#D4D4D4", "#CCCCCC", "#BBBBBB", "#AAAAAA", "#999999", "#888888", "#777777",
    "#666666", "#555555", "#444444", "#333333", "#222222", "#111111", "#F0F0F0",
    "#E0E0E0", "#D0D0D0", "#C0C0C0", "#B0B0B0", "#A0A0A0", "#909090", "#808080",
    "#FAFAFA", "#F9F9F9", "#F7F7F7", "#F4F4F4", "#EFEFEF",
];

const GOOGLE_OAUTH_COLORS: &[&str] = &[
    "#1A73E8", "#4285F4", "#34A853", "#FBBC05", "#EA4335", "#5F6368", "#202124",
];

fn extract_colors(decls: &[CssDecl], brand_name: Option<&str>) -> Vec<BrandColor> {
    let mut counts: HashMap<String, HashMap<ColorUsage, usize>> = HashMap::new();

    for decl in decls {
        let usage = classify_property(decl.property.as_str());
        for hex in parse_colors_from_value(&decl.value) {
            if BORING_COLORS.contains(&hex.as_str()) {
                continue;
            }
            *counts.entry(hex).or_default().entry(usage.clone()).or_insert(0) += 1;
        }
    }

    let mut colors: Vec<BrandColor> = counts.into_iter().map(|(hex, usage_map)| {
        let total: usize = usage_map.values().sum();
        let usage = usage_map.into_iter().max_by_key(|(_, c)| *c).map(|(u, _)| u).unwrap_or(ColorUsage::Unknown);
        BrandColor { hex, usage, count: total }
    }).collect();
    colors.sort_by_key(|c| std::cmp::Reverse(c.count));

    // Remove Google OAuth palette from non-Google brands
    let brand = brand_name.unwrap_or("").to_ascii_lowercase();
    if !brand.contains("google") {
        let google_hits = colors.iter().filter(|c| GOOGLE_OAUTH_COLORS.contains(&c.hex.as_str())).count();
        if google_hits >= 3 {
            colors.retain(|c| !GOOGLE_OAUTH_COLORS.contains(&c.hex.as_str()));
        }
    }

    // Assign Primary/Secondary to top Unknown colors
    let mut primary_assigned = colors.iter().any(|c| c.usage == ColorUsage::Primary);
    let mut secondary_assigned = colors.iter().any(|c| c.usage == ColorUsage::Secondary);
    for color in &mut colors {
        if color.usage != ColorUsage::Unknown { continue; }
        if !primary_assigned { color.usage = ColorUsage::Primary; primary_assigned = true; }
        else if !secondary_assigned { color.usage = ColorUsage::Secondary; secondary_assigned = true; }
    }

    colors.truncate(10);
    colors
}

fn classify_property(property: &str) -> ColorUsage {
    match property {
        "background-color" | "background" => ColorUsage::Background,
        "color" => ColorUsage::Text,
        "border-color" | "border" | "outline-color" => ColorUsage::Accent,
        _ => ColorUsage::Unknown,
    }
}

fn parse_colors_from_value(value: &str) -> Vec<String> {
    let mut colors = Vec::new();

    for cap in HEX_COLOR.captures_iter(value) {
        if let Some(short) = cap.get(1) {
            colors.push(expand_short_hex(short.as_str()));
        } else if let Some(full) = cap.get(2) {
            colors.push(format!("#{}", full.as_str().to_ascii_uppercase()));
        }
    }

    for cap in RGB_COLOR.captures_iter(value) {
        let r: u8 = cap[1].parse().unwrap_or(0);
        let g: u8 = cap[2].parse().unwrap_or(0);
        let b: u8 = cap[3].parse().unwrap_or(0);
        colors.push(format!("#{r:02X}{g:02X}{b:02X}"));
    }

    for cap in RGBA_COLOR.captures_iter(value) {
        let r: u8 = cap[1].parse().unwrap_or(0);
        let g: u8 = cap[2].parse().unwrap_or(0);
        let b: u8 = cap[3].parse().unwrap_or(0);
        colors.push(format!("#{r:02X}{g:02X}{b:02X}"));
    }

    for cap in HSL_COLOR.captures_iter(value) {
        let h: f64 = cap[1].parse().unwrap_or(0.0);
        let s: f64 = cap[2].parse::<f64>().unwrap_or(0.0) / 100.0;
        let l: f64 = cap[3].parse::<f64>().unwrap_or(0.0) / 100.0;
        let (r, g, b) = hsl_to_rgb(h, s, l);
        colors.push(format!("#{r:02X}{g:02X}{b:02X}"));
    }

    colors
}

fn expand_short_hex(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    format!("#{0}{0}{1}{1}{2}{2}", chars[0], chars[1], chars[2]).to_ascii_uppercase()
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let h = h / 360.0;
    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    ((r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8)
}

fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 0.5 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    p
}

// ── Font extraction ───────────────────────────────────────────────────────────

const GENERIC_FONTS: &[&str] = &[
    "serif", "sans-serif", "monospace", "cursive", "fantasy", "system-ui",
    "ui-serif", "ui-sans-serif", "ui-monospace", "ui-rounded", "emoji", "math",
    "fangsong", "inherit", "initial", "unset", "revert",
    "arial", "times", "times new roman", "courier new", "georgia", "menlo",
    "monaco", "consolas", "liberation mono", "sf mono", "sfmono-regular",
    "source code pro", "apple color emoji", "segoe ui", "segoe ui emoji",
    "segoe ui symbol", "noto color emoji", "blinkmacsystemfont", "-apple-system",
];

fn extract_fonts(decls: &[CssDecl], brand_name: Option<&str>) -> Vec<String> {
    let mut freq: HashMap<String, usize> = HashMap::new();
    let brand = brand_name.unwrap_or("").to_ascii_lowercase();

    for decl in decls {
        if decl.property != "font-family" && decl.property != "font" { continue; }

        let family_str = if decl.property == "font" {
            match parse_font_shorthand_family(&decl.value) {
                Some(f) => f,
                None => continue,
            }
        } else {
            decl.value.clone()
        };

        for font in split_font_families(&family_str) {
            let lower = font.to_lowercase();
            if !GENERIC_FONTS.contains(&lower.as_str())
                && !is_junk_font(&lower)
                && !(brand.contains("google") == false && lower.contains("google sans"))
            {
                *freq.entry(font).or_insert(0) += 1;
            }
        }
    }

    let mut fonts: Vec<(String, usize)> = freq.into_iter().collect();
    fonts.sort_by_key(|f| std::cmp::Reverse(f.1));
    fonts.into_iter().map(|(name, _)| name).collect()
}

fn is_junk_font(name: &str) -> bool {
    if name.starts_with("var(") { return true; }
    if name.len() >= 8 && name.chars().all(|c| c.is_ascii_hexdigit()) { return true; }
    if name.len() < 3 { return true; }
    if name.contains("katex") || name.contains("icon") || name.contains("emoji") || name.contains("symbol") { return true; }
    if name.contains(')') || name.contains('!') || name.contains("px ") || name.contains("rem ") { return true; }
    if name.starts_with('_') || name.starts_with("--") { return true; }
    false
}

fn parse_font_shorthand_family(value: &str) -> Option<String> {
    let caps = FONT_SHORTHAND_FAMILY.captures(value)?;
    let family = caps.get(1)?.as_str().trim().to_string();
    if family.is_empty() { None } else { Some(family) }
}

fn split_font_families(value: &str) -> Vec<String> {
    value.split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn extract_font_name_from_url(url: &str) -> Option<String> {
    let filename = url.rsplit('/').next()?;
    let stem = filename.split('.').next()?;
    let clean = stem.split('-')
        .take_while(|p| !matches!(p.to_lowercase().as_str(), "regular" | "bold" | "italic" | "light" | "medium" | "semibold" | "variable" | "subset" | "latin"))
        .collect::<Vec<_>>().join(" ");
    if clean.len() < 2 { None } else { Some(clean) }
}

fn extract_google_fonts_from_url(url: &str) -> Vec<String> {
    let mut fonts = Vec::new();
    for part in url.split('&') {
        let family = if let Some(rest) = part.strip_prefix("family=") { rest }
            else if let Some(rest) = part.split("family=").nth(1) { rest }
            else { continue };
        let name = family.split(':').next().unwrap_or(family);
        let clean = name.replace('+', " ");
        if !clean.is_empty() { fonts.push(clean); }
    }
    fonts
}

// ── Logo detection ────────────────────────────────────────────────────────────

fn find_logo(doc: &Html, base_url: Option<&Url>) -> Option<String> {
    for el in doc.select(sel!("header img, nav img")) {
        let class = el.value().attr("class").unwrap_or("");
        let id = el.value().attr("id").unwrap_or("");
        let alt = el.value().attr("alt").unwrap_or("");
        let src = el.value().attr("src")?;
        if ci_contains(class, "logo") || ci_contains(id, "logo") || ci_contains(alt, "logo") {
            return Some(resolve_url(src, base_url));
        }
    }

    for el in doc.select(sel!("a[href='/'] img, a[href] img")) {
        if let Some(parent) = el.parent().and_then(|p| p.value().as_element()) {
            let href = parent.attr("href").unwrap_or("");
            if href == "/" || href.ends_with(".com") || href.ends_with(".com/") {
                if let Some(src) = el.value().attr("src") {
                    return Some(resolve_url(src, base_url));
                }
            }
        }
    }

    None
}

fn find_favicon(doc: &Html, base_url: Option<&Url>) -> Option<String> {
    doc.select(sel!("link[rel]"))
        .find(|el| el.value().attr("rel").is_some_and(|r| r.to_lowercase().contains("icon")))
        .and_then(|el| el.value().attr("href"))
        .map(|href| resolve_url(href, base_url))
}

fn find_og_image(doc: &Html, base_url: Option<&Url>) -> Option<String> {
    doc.select(sel!("meta[property='og:image']"))
        .find_map(|el| el.value().attr("content").filter(|c| !c.is_empty()))
        .or_else(|| {
            doc.select(sel!("meta[name='twitter:image']"))
                .find_map(|el| el.value().attr("content").filter(|c| !c.is_empty()))
        })
        .map(|src| resolve_url(src, base_url))
}

fn find_all_logos(doc: &Html, base_url: Option<&Url>) -> Vec<LogoVariant> {
    let mut logos = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut add = |url: String, kind: &str| {
        if !url.is_empty() && seen.insert(url.clone()) {
            logos.push(LogoVariant { url, kind: kind.to_string() });
        }
    };

    for el in doc.select(sel!("link[rel]")) {
        let rel = el.value().attr("rel").unwrap_or("").to_lowercase();
        if let Some(href) = el.value().attr("href") {
            if rel.contains("icon") && !rel.contains("apple") {
                add(resolve_url(href, base_url), "favicon");
            }
        }
    }

    for el in doc.select(sel!("link[rel='apple-touch-icon']")) {
        if let Some(href) = el.value().attr("href") {
            add(resolve_url(href, base_url), "apple-touch-icon");
        }
    }

    for el in doc.select(sel!("header img, nav img")) {
        let class = el.value().attr("class").unwrap_or("");
        let id = el.value().attr("id").unwrap_or("");
        let alt = el.value().attr("alt").unwrap_or("");
        if (ci_contains(class, "logo") || ci_contains(id, "logo") || ci_contains(alt, "logo")) {
            if let Some(src) = el.value().attr("src") {
                add(resolve_url(src, base_url), "logo");
            }
        }
    }

    logos
}

// ── Brand name ───────────────────────────────────────────────────────────────

fn extract_brand_name(doc: &Html) -> Option<String> {
    for el in doc.select(sel!("meta[property='og:site_name']")) {
        if let Some(c) = el.value().attr("content") {
            let n = c.trim();
            if !n.is_empty() { return Some(n.to_string()); }
        }
    }

    for el in doc.select(sel!("meta[name='application-name']")) {
        if let Some(c) = el.value().attr("content") {
            let n = c.trim();
            if !n.is_empty() { return Some(n.to_string()); }
        }
    }

    for el in doc.select(sel!("title")) {
        let title: String = el.text().collect();
        let t = title.trim();
        if !t.is_empty() { return Some(clean_title(t)); }
    }

    None
}

fn clean_title(title: &str) -> String {
    for sep in [" | ", " - ", " — ", " · "] {
        if let Some(pos) = title.find(sep) {
            let left = title[..pos].trim();
            let right = title[pos + sep.len()..].trim();
            if right.len() < left.len() && right.len() >= 2 {
                return right.to_string();
            }
            return left.to_string();
        }
    }
    title.to_string()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn ci_contains(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn resolve_url(src: &str, base_url: Option<&Url>) -> String {
    match base_url {
        Some(base) => base.join(src).map(|u| u.to_string()).unwrap_or_else(|_| src.to_string()),
        None => src.to_string(),
    }
}

#[cfg(test)]
#[path = "brand_tests.rs"]
mod tests;
```

- [ ] **Step 4: Register module in `src/services.rs`**

Add `pub mod brand;` alongside other service modules (alphabetically, near `ask`).

- [ ] **Step 5: Run the brand tests (green phase)**

```bash
rtk cargo test brand_tests 2>&1 | tail -30
```

Expected: all 9 tests pass. Fix any compile errors — common issues are selector strings or regex escape sequences. Re-run until green.

- [ ] **Step 6: Commit**

```bash
rtk git add src/services/brand.rs src/services/brand_tests.rs src/services.rs
rtk git commit -m "feat(services): implement brand service with DOM/CSS extraction and sidecar tests"
```

---

## Task 7: Implement `src/cli/commands/brand.rs` CLI handler

**Files:**
- Create: `src/cli/commands/brand.rs`
- Create: `src/cli/commands/brand_tests.rs`

- [ ] **Step 1: Write the sidecar test file first (red phase)**

Create `src/cli/commands/brand_tests.rs`:

```rust
use super::*;
use crate::services::types::{BrandColor, BrandResult, ColorUsage, LogoVariant};

fn make_brand_result() -> BrandResult {
    BrandResult {
        url: "https://example.com".to_string(),
        name: Some("Acme Corp".to_string()),
        colors: vec![
            BrandColor { hex: "#3498DB".to_string(), usage: ColorUsage::Primary, count: 5 },
            BrandColor { hex: "#2ECC71".to_string(), usage: ColorUsage::Secondary, count: 3 },
        ],
        fonts: vec!["Inter".to_string(), "Fira Code".to_string()],
        logos: vec![LogoVariant { url: "https://example.com/logo.svg".to_string(), kind: "logo".to_string() }],
        logo_url: Some("https://example.com/logo.svg".to_string()),
        favicon_url: Some("https://example.com/favicon.ico".to_string()),
        og_image: None,
    }
}

#[test]
fn test_format_brand_summary_contains_name() {
    let result = make_brand_result();
    let output = format_brand_summary(&result);
    assert!(output.contains("Acme Corp"), "should include brand name, got: {output}");
}

#[test]
fn test_format_brand_summary_contains_color_count() {
    let result = make_brand_result();
    let output = format_brand_summary(&result);
    assert!(output.contains('2') || output.contains("color"), "should mention color count, got: {output}");
}

#[test]
fn test_format_brand_summary_contains_font_name() {
    let result = make_brand_result();
    let output = format_brand_summary(&result);
    assert!(output.contains("Inter"), "should include font names, got: {output}");
}
```

- [ ] **Step 2: Run the test to confirm it fails (red)**

```bash
rtk cargo test brand_tests -- --test-output immediate 2>&1 | tail -10
```

Expected: compile error — `format_brand_summary` not found.

- [ ] **Step 3: Implement `src/cli/commands/brand.rs`**

Create `src/cli/commands/brand.rs`:

```rust
use crate::core::config::Config;
use crate::core::logging::{log_done, log_info};
use crate::core::ui::{accent, muted, primary, print_option, print_phase};
use crate::services::brand as brand_svc;
use crate::services::types::{BrandResult, ColorUsage};
use std::error::Error;

pub async fn run_brand(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let url = parse_brand_url(cfg)?;

    log_info(&format!("command=brand url={url}"));
    let result = brand_svc::brand(cfg, &url, None).await?;

    emit_brand_result(cfg, &result)?;

    log_done(&format!(
        "command=brand url={url} colors={} fonts={} logos={}",
        result.colors.len(),
        result.fonts.len(),
        result.logos.len(),
    ));
    Ok(())
}

fn parse_brand_url(cfg: &Config) -> Result<String, Box<dyn Error>> {
    cfg.positional
        .first()
        .cloned()
        .or_else(|| cfg.start_url.clone())
        .ok_or_else(|| "brand requires a URL: axon brand <url>".into())
}

pub(crate) fn emit_brand_result(cfg: &Config, result: &BrandResult) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(());
    }

    print_phase("◐", "Brand", &result.url);
    if let Some(ref name) = result.name {
        print_option("name", name);
    }
    print_option("colors", &result.colors.len().to_string());
    print_option("fonts", &result.fonts.len().to_string());
    print_option("logos", &result.logos.len().to_string());

    if !result.colors.is_empty() {
        println!("\n{}", primary("Brand Colors"));
        for color in &result.colors {
            let usage_label = match color.usage {
                ColorUsage::Primary => "primary",
                ColorUsage::Secondary => "secondary",
                ColorUsage::Background => "background",
                ColorUsage::Text => "text",
                ColorUsage::Accent => "accent",
                ColorUsage::Unknown => "unknown",
            };
            println!("  {} {} {} ({})", muted("•"), accent(&color.hex), muted(usage_label), color.count);
        }
    }

    if !result.fonts.is_empty() {
        println!("\n{}", primary("Fonts"));
        for font in &result.fonts {
            println!("  {} {}", muted("•"), font);
        }
    }

    if let Some(ref logo) = result.logo_url {
        println!("\n{}", primary("Logo"));
        println!("  {logo}");
    }

    if let Some(ref favicon) = result.favicon_url {
        println!("\n{}", primary("Favicon"));
        println!("  {favicon}");
    }

    if !result.logos.is_empty() {
        println!("\n{}", primary("All Logo Variants"));
        for logo in &result.logos {
            println!("  {} {} ({})", muted("•"), logo.url, logo.kind);
        }
    }

    Ok(())
}

/// Pure formatting helper exposed for testing.
pub(crate) fn format_brand_summary(result: &BrandResult) -> String {
    let name = result.name.as_deref().unwrap_or("(unknown)");
    let fonts = result.fonts.join(", ");
    format!(
        "name={name} colors={} fonts=[{fonts}] logos={}",
        result.colors.len(),
        result.logos.len(),
    )
}

#[cfg(test)]
#[path = "brand_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run the tests (green phase)**

```bash
rtk cargo test brand_tests 2>&1 | tail -20
```

Expected: all 3 tests pass.

- [ ] **Step 5: Commit**

```bash
rtk git add src/cli/commands/brand.rs src/cli/commands/brand_tests.rs
rtk git commit -m "feat(cli): add brand command handler with human+JSON output formatting"
```

---

## Task 8: Wire `brand` into CLI, `CommandKind`, `lib.rs`, and MCP

**Files:** Same set as Task 5 but for `brand`.

- [ ] **Step 1: Add `run_brand` to `src/cli/commands.rs`**

```rust
pub mod brand;
pub use brand::run_brand;
```

- [ ] **Step 2: Add `CommandKind::Brand` to `src/core/config/types/enums.rs`**

In the enum:

```rust
Brand,
```

In `as_str()`:

```rust
Self::Brand => "brand",
```

- [ ] **Step 3: Add `Brand(ScrapeArgs)` to `src/core/config/cli.rs`**

`brand` takes one URL via positional, same as `summarize`. Use the existing `ScrapeArgs` struct:

```rust
/// Analyze a URL's brand identity: colors, fonts, logos, favicon
Brand(ScrapeArgs),
```

No new `Args` struct needed — `ScrapeArgs` already has `positional_urls: Vec<String>`.

- [ ] **Step 4: Map `CliCommand::Brand` in `command_dispatch.rs`**

```rust
CliCommand::Brand(args) => {
    out.command = CommandKind::Brand;
    out.positional = args.positional_urls;
}
```

- [ ] **Step 5: Add dispatch arm to `src/lib.rs`**

Import `run_brand`. Add:

```rust
CommandKind::Brand => run_brand(cfg).await?,
```

- [ ] **Step 6: Add `BrandRequest` to `src/mcp/schema/utility.rs`**

Append:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrandRequest {
    /// URL to analyze
    pub url: Option<String>,
    /// Rendering engine override
    pub render_mode: Option<McpRenderMode>,
    pub response_mode: Option<ResponseMode>,
}
```

Also add `use super::requests::{McpRenderMode, ResponseMode, ...};` if `McpRenderMode` is not already imported in `utility.rs` (check the existing import at line 4).

- [ ] **Step 7: Add `Brand(BrandRequest)` to `src/mcp/schema.rs`**

In `AxonRequest`:

```rust
Brand(BrandRequest),
```

Ensure `BrandRequest` is accessible (it's in `utility.rs` which is already re-exported via `pub use utility::*;`).

- [ ] **Step 8: Add brand to `src/services/action_api.rs`**

In the dispatch match:

```rust
AxonRequest::Brand(req) => commands::dispatch_brand(service_context, req).await,
```

In `required_scope`:

```rust
AxonRequest::Brand(_) => Some("axon:read"),
```

In `action_name`:

```rust
AxonRequest::Brand(_) => "brand",
```

- [ ] **Step 9: Add `dispatch_brand` to `src/services/action_api/commands/dispatchers.rs`**

Import:

```rust
use crate::services::brand as brand_svc;
use crate::mcp::schema::BrandRequest;
```

Function:

```rust
pub async fn dispatch_brand(
    service_context: &ServiceContext,
    req: BrandRequest,
) -> Result<serde_json::Value, ClientActionError> {
    let url = req.url.ok_or_else(|| {
        ClientActionError::new("invalid_request", "url is required for brand", false, None)
    })?;

    let result = brand_svc::brand(service_context.cfg.as_ref(), &url, None)
        .await
        .map_err(internal_error)?;

    serde_json::to_value(result)
        .map_err(|e| ClientActionError::new("internal_error", format!("serialize brand result: {e}"), false, None))
}
```

- [ ] **Step 10: Re-export from `src/services/action_api/commands.rs`**

Add `dispatch_brand` to the `pub(super) use dispatchers::{...}` line.

- [ ] **Step 11: Add `handle_brand` to `src/mcp/server/handlers_query.rs`**

Import: `use crate::mcp::schema::BrandRequest;` and `use crate::services::brand as brand_svc;`

```rust
pub(super) async fn handle_brand(
    service_context: &ServiceContext,
    req: BrandRequest,
) -> Result<AxonToolResponse, ErrorData> {
    let url = req
        .url
        .clone()
        .ok_or_else(|| invalid_params("url is required for brand"))?;

    let result = brand_svc::brand(service_context.cfg.as_ref(), &url, None)
        .await
        .map_err(|e| logged_internal_error("brand", e.as_ref()))?;

    let data = serde_json::to_value(&result)
        .map_err(|e| internal_error(format!("serialize brand result: {e}")))?;

    Ok(AxonToolResponse::ok("brand", "brand", data))
}
```

Wire into `src/mcp/server.rs`:

```rust
AxonRequest::Brand(req) => {
    handlers_query::handle_brand(&self.service_context, req).await
}
```

- [ ] **Step 12: Update help in `src/mcp/server/handlers_system.rs`**

Add `"brand"` to the action list.

- [ ] **Step 13: Verify compilation**

```bash
rtk cargo check --bin axon 2>&1 | head -40
```

Expected: no errors. All enum match arms must be exhaustive — if the compiler complains about a missing arm in a match on `AxonRequest` or `CommandKind`, add the missing arm.

- [ ] **Step 14: Run all brand tests**

```bash
rtk cargo test brand 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 15: Commit**

```bash
rtk git add \
  src/cli/commands.rs \
  src/core/config/types/enums.rs \
  src/core/config/cli.rs \
  src/core/config/parse/build_config/command_dispatch.rs \
  src/lib.rs \
  src/mcp/schema/utility.rs \
  src/mcp/schema.rs \
  src/services/action_api.rs \
  src/services/action_api/commands/dispatchers.rs \
  src/services/action_api/commands.rs \
  src/mcp/server/handlers_query.rs \
  src/mcp/server/handlers_system.rs
rtk git commit -m "feat(brand): wire brand command through CLI, CommandKind, lib.rs, and MCP"
```

---

## Task 9: Docs, CLAUDE.md, and final verification

**Files:**
- Modify: `docs/MCP-TOOL-SCHEMA.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update `docs/MCP-TOOL-SCHEMA.md`**

Find the section listing available actions (look for `### summarize`). Add two new action sections after it:

```markdown
### `diff`

Compare two URLs and return a structured diff.

**Request:**
```json
{
  "action": "diff",
  "url_a": "https://example.com/v1",
  "url_b": "https://example.com/v2",
  "render_mode": "http"
}
```

**Response** (`DiffResult`):
- `url_a`, `url_b` — the two compared URLs
- `status` — `"same"` or `"changed"`
- `text_diff` — unified diff of markdown content (null when `status == "same"`)
- `metadata_changes` — array of `{field, old, new}` for title/description/etc.
- `links_added`, `links_removed` — arrays of `{href, text}`
- `word_count_delta` — integer (positive = B has more words)

---

### `brand`

Extract brand identity from a URL: colors, fonts, logos, favicon.

**Request:**
```json
{
  "action": "brand",
  "url": "https://example.com"
}
```

**Response** (`BrandResult`):
- `url` — the analyzed URL
- `name` — brand name (from og:site_name, application-name, or `<title>`)
- `colors` — up to 10 `{hex, usage, count}` entries (usage: primary/secondary/background/text/accent/unknown)
- `fonts` — ordered list of brand-specific font family names
- `logos` — all logo variants `{url, kind}` (kind: favicon/apple-touch-icon/logo/og-image/svg)
- `logo_url` — primary logo URL (best guess)
- `favicon_url` — favicon URL
- `og_image` — Open Graph image URL
```

- [ ] **Step 2: Update `CLAUDE.md` command table**

Find the table under `## Commands` and add two rows:

```markdown
| `diff <url-a> <url-b>` | Compare two URLs, show content/metadata/link changes | No |
| `brand <url>` | Extract brand identity: colors, fonts, logos, favicon | No |
```

- [ ] **Step 3: Run the full test suite**

```bash
rtk cargo test 2>&1 | tail -40
```

Expected: all tests pass, no regressions.

- [ ] **Step 4: Run the monolith check**

```bash
./scripts/check-monolith.sh 2>&1 | tail -20
```

Or if the script path is different:

```bash
just verify 2>&1 | tail -40
```

Expected: all files within 500-line limit, no function exceeding 120 lines. If `brand.rs` is over the limit, split the font extraction helpers into `src/services/brand/fonts.rs` and the color extraction into `src/services/brand/colors.rs`, keeping `brand.rs` as the module root that `pub use`s from those submodules. Update the `#[path]` declaration accordingly — the sidecar test file is `brand_tests.rs` next to `brand.rs`.

- [ ] **Step 5: Commit docs**

```bash
rtk git add docs/MCP-TOOL-SCHEMA.md CLAUDE.md
rtk git commit -m "docs: add diff and brand to MCP schema docs and CLAUDE.md command table"
```

- [ ] **Step 6: Final push**

```bash
rtk git pull --rebase
rtk git push
```

---

## Bead Structure Recommendation

The puoi bead (`axon_rust-puoi`) covers all three tools. Given that `summarize` is already done, the recommended breakdown for tracking remaining work is:

- **No split needed.** Both `diff` and `brand` are straightforward enough to complete in one session following this plan. If you prefer granularity, create two child beads:
  - `axon_rust-puoi-diff` — Tasks 1–5 of this plan
  - `axon_rust-puoi-brand` — Tasks 6–9 of this plan
  - Close the parent `axon_rust-puoi` when both children are closed.

---

## Self-Review Checklist

**Spec coverage:**
- [x] `diff` CLI command with two positional URL args — Task 5, `DiffArgs`
- [x] `brand` CLI command with one positional URL — Task 8, uses `ScrapeArgs`
- [x] Service functions: `diff::diff()`, `brand::brand()` — Tasks 3, 6
- [x] Pure computation functions for testability — `compute_diff`, `extract_brand_from_html`
- [x] Typed result structs in `service.rs` — Task 2
- [x] MCP request structs: `DiffRequest`, `BrandRequest` — Tasks 5, 8
- [x] `AxonRequest` variants: `Diff`, `Brand` — Tasks 5, 8
- [x] MCP dispatcher functions: `dispatch_diff`, `dispatch_brand` — Tasks 5, 8
- [x] MCP handler functions: `handle_diff`, `handle_brand` — Tasks 5, 8
- [x] CLI output formatters (human and JSON) — Tasks 4, 7
- [x] Sidecar test files for all new source files — Tasks 3, 4, 6, 7
- [x] `CommandKind` enum variants — Tasks 5, 8
- [x] `CliCommand` enum variants — Tasks 5, 8
- [x] `command_dispatch.rs` arms — Tasks 5, 8
- [x] `lib.rs` dispatch arms — Tasks 5, 8
- [x] `services/action_api.rs` dispatch, scope, name — Tasks 5, 8
- [x] Help action updated — Tasks 5, 8
- [x] MCP schema docs updated — Task 9
- [x] CLAUDE.md command table updated — Task 9
- [x] New dependencies added — Task 1

**Type consistency check:**
- `DiffResult`, `DiffStatus`, `MetadataChange`, `LinkEntry` defined in Task 2, used in Tasks 3, 4, 5
- `BrandResult`, `BrandColor`, `ColorUsage`, `LogoVariant` defined in Task 2, used in Tasks 6, 7, 8
- `compute_diff` signature in Task 3 matches test calls in `diff_tests.rs`
- `extract_brand_from_html` signature in Task 6 matches test calls in `brand_tests.rs`
- `format_diff_summary` in Task 4 matches test calls in `diff_tests.rs`
- `format_brand_summary` in Task 7 matches test calls in `brand_tests.rs`
- `dispatch_diff` / `dispatch_brand` take `&ServiceContext` — consistent with other dispatchers
- `handle_diff` / `handle_brand` return `Result<AxonToolResponse, ErrorData>` — consistent with other handlers

**Placeholder scan:** No TBD, TODO, or "implement later" patterns. Every step has either code or an exact command with expected output.
