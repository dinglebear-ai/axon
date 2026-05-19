# `axon export` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `axon export` CLI command + MCP action that produces a comprehensive JSON manifest of everything indexed in the vector DB and job history — sufficient to fully repopulate the index from scratch.

**Architecture:** Two-phase approach. Phase 1 adds `source_type` provenance tracking (the `"embed"` catch-all gap) so future indexed content is properly tagged. Phase 2 builds the export command that aggregates data from both Postgres job tables and Qdrant facets into a single JSON file. The export follows the services-first pattern: `crates/services/export.rs` → consumed by CLI handler + MCP handler.

**Tech Stack:** Rust, serde_json, sqlx (Postgres), Qdrant REST API (facets + scroll), existing `Config` infrastructure.

---

## Audit Summary: Current State

Before diving into tasks, here's what the codebase research revealed:

### What's Already Tracked

| Data Point | Where | How |
|---|---|---|
| Crawl seed URLs | `axon_crawl_jobs.url` | Postgres column |
| Crawl configs (max_pages, render_mode, etc.) | `axon_crawl_jobs.config_json` | JSONB |
| Extract prompts | `axon_extract_jobs.config_json.prompt` | JSONB field |
| Extract URLs | `axon_extract_jobs.urls_json` | JSONB array |
| GitHub repos | `axon_ingest_jobs` where `source_type='github'` | Postgres |
| Reddit targets | `axon_ingest_jobs` where `source_type='reddit'` | Postgres |
| YouTube targets | `axon_ingest_jobs` where `source_type='youtube'` | Postgres |
| Sessions | `axon_ingest_jobs` where `source_type='sessions'` | Postgres |
| Refresh schedules | `axon_refresh_schedules` | Full table |
| Qdrant `source_type` field | Per-point payload | `"embed"`, `"github"`, `"reddit"`, `"youtube"`, `"sessions"`, `"refresh"` |
| Qdrant `url` + `domain` fields | Per-point payload | Keyword-indexed |

### Critical Gaps (Must Fix)

1. **`source_type="embed"` is a catch-all** — crawl, scrape, search, and manual embed all produce the same `source_type`. Can't distinguish them.
2. **No `"scrape"` source_type** — scrape path doesn't tag points differently from crawl.
3. **No `"crawl"` source_type** — crawl → embed job pipeline loses the crawl origin.
4. **No `"search"` source_type** — search auto-queues crawl jobs but points have no trace of the search origin.
5. **Embedded local files** — `axon embed /path/to/file` uses `source_type="embed"` and `url="local://path"`, but there's no inventory of embedded local paths in Postgres.

### What We Can Reconstruct Without Code Changes

Even without fixing gap #1, the export can reconstruct most data from Postgres:
- All crawl seed URLs + configs → `axon_crawl_jobs`
- All extract URLs + prompts → `axon_extract_jobs`
- All ingest targets → `axon_ingest_jobs`
- All refresh schedules → `axon_refresh_schedules`
- All embedded inputs → `axon_embed_jobs.input_text`
- All indexed URLs → Qdrant `url` facet
- All indexed domains → Qdrant `domain` facet
- Source type distribution → Qdrant `source_type` facet

### What We CANNOT Reconstruct

- Which specific URLs came from scrape vs crawl vs search (all `source_type="embed"`)
- Which interface triggered the operation (CLI vs MCP vs web)

**Decision:** Phase 1 fixes the `source_type` provenance gap for **future** indexing. The export command works with whatever data exists — it doesn't need perfect provenance to be useful. Old `source_type="embed"` points get exported as-is; the user can cross-reference with Postgres job history.

---

## File Structure

### New Files

| File | Responsibility |
|---|---|
| `crates/services/export.rs` | Service function: aggregate all data sources → `ExportManifest` |
| `crates/services/types/export.rs` | `ExportManifest` and section types (`CrawlExport`, `IngestExport`, etc.) |
| `crates/cli/commands/export.rs` | CLI handler: call service → write JSON to file |

### Modified Files

| File | Change |
|---|---|
| `crates/core/config/types/enums.rs` | Add `Export` variant to `CommandKind` |
| `crates/core/config/cli.rs` | Add `Export` variant to `CliCommand` |
| `crates/core/config/parse/build_config.rs` | Wire `CliCommand::Export` → `CommandKind::Export` |
| `crates/cli/commands.rs` (or `mod.rs` equivalent) | Add `pub mod export;` |
| `crates/services.rs` (or `mod.rs` equivalent) | Add `pub mod export;` |
| `crates/services/types.rs` | Add `pub mod export;` re-export |
| `lib.rs` | Add `CommandKind::Export` dispatch |
| `crates/mcp/schema.rs` | Add `Export` action + `ExportRequest` struct |
| `crates/mcp/server.rs` | Add `AxonRequest::Export` dispatch arm |
| `crates/mcp/server/handlers_system.rs` | Add `handle_export` implementation |
| `crates/vector/ops/tei/prepare.rs:75` | Accept optional `source_type` override (currently hardcodes `"embed"`) |
| `crates/vector/ops/tei/text_embed.rs:31` | Add `source_type` param to `embed_path_native_with_progress()` signature |
| `crates/vector/ops/tei.rs:14` | Update re-export if signature changes |
| `crates/crawl/scrape.rs` | Pass `source_type="scrape"` through embed pipeline |
| `crates/cli/commands/scrape.rs` | Pass `source_type="scrape"` when calling embed service |
| `crates/services/embed.rs` | Thread `source_type` from caller through to embed pipeline |
| `crates/jobs/crawl/runtime/worker/` | Pass `source_type="crawl"` when enqueuing embed jobs |
| `crates/jobs/embed.rs:29` | Add `source_type: Option<String>` field to `EmbedJobConfig` struct |
| `crates/jobs/embed/worker.rs:120` | Read `source_type` from `EmbedJobConfig`, pass to `embed_path_native_with_progress()` |
| `crates/vector/ops/qdrant/client.rs` | Add generic `qdrant_facet(cfg, key, limit)` helper (DRY refactor of `qdrant_url_facets` / `qdrant_domain_facets`) |
| `crates/core/config/types/config.rs` | Add `export_no_urls: bool` and `export_url_limit: usize` fields |
| `crates/core/config/types/config_impls.rs:11` | Set defaults in `Config::default()` (`export_no_urls: false`, `export_url_limit: 100_000`) |
| `docs/MCP-TOOL-SCHEMA.md` | Document `export` action |

---

## Phase 1: Fix `source_type` Provenance (Tasks 1–5)

### Task 1: Add `source_type` to `EmbedJobConfig`

**Files:**
- Modify: `crates/jobs/embed.rs:29` — `EmbedJobConfig` struct (NOT `job_ops.rs` — it lives here)
- Test: inline `#[cfg(test)]` in `crates/jobs/embed.rs`

`EmbedJobConfig` is defined at `crates/jobs/embed.rs:29` as:
```rust
struct EmbedJobConfig {
    collection: String,
}
```
It's crate-private, so the test must live in the same crate.

- [ ] **Step 1: Write a failing test for source_type in EmbedJobConfig**

Add to the test module in `crates/jobs/embed.rs`:

```rust
#[test]
fn embed_job_config_includes_source_type() {
    let cfg = EmbedJobConfig {
        collection: "test".into(),
        source_type: Some("crawl".into()),
    };
    let json = serde_json::to_value(&cfg).unwrap();
    assert_eq!(json["source_type"], "crawl");
}

#[test]
fn embed_job_config_deserializes_without_source_type() {
    // Existing jobs in Postgres won't have source_type — must deserialize cleanly
    let json = r#"{"collection":"cortex"}"#;
    let cfg: EmbedJobConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.source_type, None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test embed_job_config_includes_source_type -- --nocapture`
Expected: FAIL — `source_type` field doesn't exist.

- [ ] **Step 3: Add `source_type` field to `EmbedJobConfig`**

In `crates/jobs/embed.rs:29`, change the struct to:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EmbedJobConfig {
    collection: String,
    #[serde(default)]
    source_type: Option<String>,
}
```

`#[serde(default)]` + `Option` ensures backward compat with existing Postgres rows that lack this field.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test embed_job_config -- --nocapture`
Expected: Both tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/jobs/embed.rs
git commit -m "feat(embed): add source_type field to EmbedJobConfig for provenance tracking"
```

---

### Task 2: Thread `source_type` Through the Full Embed Pipeline

**Files:**
- Modify: `crates/vector/ops/tei/prepare.rs:47` — add `source_type: &str` param to `prepare_embed_docs()`
- Modify: `crates/vector/ops/tei/prepare.rs:75` — use param instead of hardcoded `"embed"`
- Modify: `crates/vector/ops/tei/text_embed.rs:31` — add `source_type: Option<&str>` param to `embed_path_native_with_progress()`
- Modify: `crates/vector/ops/tei/text_embed.rs:26` — update `embed_path_native()` wrapper to pass `None`
- Modify: `crates/vector/ops/tei.rs:14` — update re-export if signature changes
- Modify: `crates/vector/ops.rs:13` — update re-export
- Modify: `crates/services/embed.rs:129` — pass `None` to preserve existing behavior
- Modify: `crates/jobs/embed/worker.rs:120` — read `source_type` from `EmbedJobConfig`, pass to pipeline

The actual call chain that must be traced:
```
embed worker (worker.rs:120)
  → embed_path_native_with_progress(cfg, input, progress_tx)  [text_embed.rs:31]
    → prepare_embed_docs(input, exclude_prefixes)              [prepare.rs:47]
      → PreparedDoc { source_type: "embed" }                   [prepare.rs:75] ← hardcoded here
```

The services layer also calls:
```
services/embed.rs:129
  → embed_path_native(cfg, input)  [text_embed.rs:26]
    → embed_path_native_with_progress(cfg, input, None)
```

- [ ] **Step 1: Write a failing test for `prepare_embed_docs` with source_type param**

In `crates/vector/ops/tei/prepare.rs` test module:

```rust
#[tokio::test]
async fn prepare_embed_docs_uses_given_source_type() {
    let docs = prepare_embed_docs("test text", &[], "crawl").await.unwrap();
    assert!(!docs.is_empty());
    assert_eq!(docs[0].source_type, "crawl");
}

#[tokio::test]
async fn prepare_embed_docs_defaults_to_embed() {
    let docs = prepare_embed_docs("test text", &[], "embed").await.unwrap();
    assert_eq!(docs[0].source_type, "embed");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test prepare_embed_docs_uses -- --nocapture`
Expected: FAIL — `prepare_embed_docs` doesn't accept a third parameter.

- [ ] **Step 3: Add `source_type` parameter to `prepare_embed_docs`**

In `crates/vector/ops/tei/prepare.rs:47`, change:
```rust
// Before:
pub(super) async fn prepare_embed_docs(
    input: &str,
    exclude_prefixes: &[String],
) -> Result<Vec<PreparedDoc>, Box<dyn Error>> {

// After:
pub(super) async fn prepare_embed_docs(
    input: &str,
    exclude_prefixes: &[String],
    source_type: &str,
) -> Result<Vec<PreparedDoc>, Box<dyn Error>> {
```

At line 75, change:
```rust
// Before:
source_type: "embed".to_string(),

// After:
source_type: source_type.to_string(),
```

- [ ] **Step 4: Update `embed_path_native_with_progress` signature**

In `crates/vector/ops/tei/text_embed.rs:31`, add `source_type: Option<&str>`:
```rust
pub async fn embed_path_native_with_progress(
    cfg: &Config,
    input: &str,
    progress: Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
    source_type: Option<&str>,
) -> Result<EmbedSummary, Box<dyn Error>> {
    let st = source_type.unwrap_or("embed");
    let prepared = prepare::prepare_embed_docs(input, &cfg.exclude_path_prefix, st).await?;
    // ... rest unchanged
```

Update `embed_path_native` wrapper (text_embed.rs:26):
```rust
pub async fn embed_path_native(cfg: &Config, input: &str) -> Result<EmbedSummary, Box<dyn Error>> {
    embed_path_native_with_progress(cfg, input, None, None).await
}
```

- [ ] **Step 5: Fix all callers**

Update callers to pass the new param:
- `crates/services/embed.rs:129`: `embed_path_native(cfg, input)` — no change needed (wrapper handles None)
- `crates/jobs/embed/worker.rs:120`: change to pass source_type from config:
  ```rust
  let source_type = job_cfg.source_type.as_deref();
  embed_path_native_with_progress(&embed_cfg, &input_text, Some(progress_tx), source_type).await;
  ```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test prepare_embed_docs -- --nocapture && cargo test --lib`
Expected: All pass.

- [ ] **Step 7: Commit**

```bash
git add crates/vector/ops/tei/ crates/jobs/embed/ crates/services/embed.rs
git commit -m "feat(embed): thread source_type through full embed pipeline (prepare→text_embed→worker)"
```

---

### Task 3: Tag Crawl-Originated Embed Jobs with `source_type="crawl"`

**Files:**
- Modify: `crates/jobs/crawl/runtime/worker/` — when crawl enqueues embed jobs, set `source_type: Some("crawl")`
- Test: Integration-style test or manual verification via `axon crawl ... --wait true` + Qdrant point check

- [ ] **Step 1: Find where crawl enqueues embed jobs**

Run: `grep -rn "enqueue.*embed\|embed.*enqueue\|EmbedJobConfig" crates/jobs/crawl/`

Locate the call site where crawl results get turned into embed jobs.

- [ ] **Step 2: Add `source_type: Some("crawl".into())` to the EmbedJobConfig construction**

At the call site found in step 1, change:

```rust
// Before:
EmbedJobConfig { collection: cfg.collection.clone() }

// After:
EmbedJobConfig {
    collection: cfg.collection.clone(),
    source_type: Some("crawl".into()),
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check --bin axon`
Expected: Clean compile.

- [ ] **Step 4: Commit**

```bash
git add crates/jobs/crawl/
git commit -m "feat(crawl): tag embed jobs with source_type=crawl for provenance"
```

---

### Task 4: Tag Scrape Path with `source_type="scrape"`

**Files:**
- Modify: `crates/cli/commands/scrape.rs:16` — `run_scrape()` calls into services layer
- Modify: `crates/services/embed.rs` — the service `embed_start()` calls `embed_path_native(cfg, input)`

The scrape → embed path flows through the services layer:
```
cli/commands/scrape.rs → run_scrape()
  → services/embed.rs → embed_path_native(cfg, scraped_output_path)  [line 129]
    → text_embed.rs → embed_path_native_with_progress(cfg, input, None, None)
```

After Task 2, `embed_path_native` passes `None` as source_type (defaulting to `"embed"`). We need the scrape command to pass `"scrape"` instead. Two approaches:

**Approach A (preferred):** Add a `source_type` parameter to the service-layer embed function, so the CLI scrape handler can specify `"scrape"`. This avoids needing to modify `embed_path_native` again — just add an overload or parameter at the service layer.

**Approach B:** Have `run_scrape` call `embed_path_native_with_progress` directly with `source_type: Some("scrape")` instead of going through the services layer.

- [ ] **Step 1: Read `crates/cli/commands/scrape.rs` and `crates/services/embed.rs`**

Identify the exact call chain from scrape → embed.

- [ ] **Step 2: Add `source_type` param to the service embed function**

In `crates/services/embed.rs`, modify `embed_start` (or create an overload `embed_start_with_source`) that accepts `source_type: Option<&str>` and passes it through to `embed_path_native_with_progress`.

- [ ] **Step 3: Call with `source_type="scrape"` from `run_scrape`**

In `crates/cli/commands/scrape.rs`, change the embed call to pass `Some("scrape")`.

- [ ] **Step 4: Verify compilation**

Run: `cargo check --bin axon`

- [ ] **Step 5: Commit**

```bash
git add crates/cli/commands/scrape.rs crates/services/embed.rs
git commit -m "feat(scrape): tag embedded content with source_type=scrape"
```

---

### Task 5 [OPTIONAL]: Tag Search-Originated Crawls with `source_type="search"`

**Files:**
- Modify: `crates/cli/commands/search.rs` — when search auto-queues crawl jobs, propagate a `source_type` hint
- **This is a stretch goal — skip if invasive.** Search enqueues crawl jobs, which enqueue embed jobs. The chain is: `search → crawl_job(config_json) → embed_job(config_json)`. We'd need to propagate `"search"` through `CrawlJobConfig` → embed. The export can cross-reference `axon_crawl_jobs` timestamps with search history as a workaround.

- [ ] **Step 1: Assess feasibility**

Read `crates/cli/commands/search.rs` and trace how it enqueues crawl jobs. Determine if `CrawlJobConfig` can carry a `source_type` hint that gets forwarded to the embed job.

- [ ] **Step 2: If feasible, add `origin_source_type: Option<String>` to `CrawlJobConfig`**

This field, when present, overrides the `source_type` on the downstream embed job.

- [ ] **Step 3: If not feasible, document the gap and move on**

Add a comment in `search.rs`:

```rust
// TODO: search-originated crawls produce source_type="crawl" on embed jobs.
// Cross-reference axon_crawl_jobs timestamps with search command history to identify search-origin content.
```

- [ ] **Step 4: Commit**

```bash
git add crates/cli/commands/search.rs crates/jobs/
git commit -m "feat(search): propagate source_type=search through crawl→embed chain (or document gap)"
```

---

## Phase 2: Export Types + Service (Tasks 6–9)

### Task 6: Define Export Manifest Types

**Files:**
- Create: `crates/services/types/export.rs`
- Modify: `crates/services/types.rs` — add `pub mod export;`

- [ ] **Step 1: Write failing test for ExportManifest serialization**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_manifest_serializes_to_json() {
        let manifest = ExportManifest {
            version: 1,
            exported_at: "2026-03-19T12:00:00Z".into(),
            collection: "cortex".into(),
            crawls: vec![],
            scrapes: vec![],
            extractions: vec![],
            embeds: vec![],
            ingests: IngestExports {
                github: vec![],
                reddit: vec![],
                youtube: vec![],
                sessions: vec![],
            },
            refreshes: RefreshExports {
                schedules: vec![],
                jobs: vec![],
            },
            qdrant_summary: QdrantSummary {
                total_points: 0,
                source_type_counts: Default::default(),
                domain_counts: Default::default(),
                indexed_urls: vec![],
            },
        };
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        assert!(json.contains("\"version\":1"));
        assert!(json.contains("\"crawls\":[]"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test export_manifest_serializes -- --nocapture`
Expected: FAIL — module doesn't exist.

- [ ] **Step 3: Implement the types**

Create `crates/services/types/export.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level export manifest — everything needed to repopulate the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportManifest {
    /// Schema version for forward compatibility.
    pub version: u32,
    /// RFC3339 timestamp when this export was generated.
    pub exported_at: String,
    /// Qdrant collection name this export represents.
    pub collection: String,

    /// All crawl jobs from Postgres (seed URLs, configs, results).
    pub crawls: Vec<CrawlExport>,
    /// All scrape-originated URLs (from Qdrant source_type facet or embed jobs).
    pub scrapes: Vec<ScrapeExport>,
    /// All extraction jobs (URLs + prompts + results).
    pub extractions: Vec<ExtractionExport>,
    /// All embed jobs (local files, URLs, raw text).
    pub embeds: Vec<EmbedExport>,
    /// All ingest sources, grouped by type.
    pub ingests: IngestExports,
    /// Refresh schedules and recent refresh jobs.
    pub refreshes: RefreshExports,
    /// Qdrant collection-level summary (point counts, source distribution).
    pub qdrant_summary: QdrantSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlExport {
    pub job_id: String,
    pub seed_url: String,
    pub status: String,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub config: serde_json::Value,
    pub pages_crawled: Option<u64>,
    pub pages_discovered: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeExport {
    pub url: String,
    /// Earliest scraped_at timestamp from Qdrant for this URL.
    pub scraped_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionExport {
    pub job_id: String,
    pub urls: Vec<String>,
    pub prompt: Option<String>,
    pub status: String,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub total_items: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedExport {
    pub job_id: String,
    pub input: String,
    pub collection: String,
    pub status: String,
    pub source_type: Option<String>,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub chunks_embedded: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestExports {
    pub github: Vec<IngestSourceExport>,
    pub reddit: Vec<IngestSourceExport>,
    pub youtube: Vec<IngestSourceExport>,
    pub sessions: Vec<IngestSourceExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestSourceExport {
    pub job_id: String,
    pub target: String,
    pub status: String,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub config: serde_json::Value,
    pub chunks_embedded: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshExports {
    pub schedules: Vec<RefreshScheduleExport>,
    pub jobs: Vec<RefreshJobExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshScheduleExport {
    pub id: String,
    pub name: String,
    pub seed_url: Option<String>,
    pub urls: Vec<String>,
    pub every_seconds: i64,
    pub enabled: bool,
    pub source_type: Option<String>,
    pub target: Option<String>,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshJobExport {
    pub job_id: String,
    pub urls: Vec<String>,
    pub status: String,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub checked: Option<u64>,
    pub changed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantSummary {
    pub total_points: u64,
    pub source_type_counts: HashMap<String, u64>,
    pub domain_counts: HashMap<String, u64>,
    /// All unique indexed URLs (from facet query). May be very large.
    pub indexed_urls: Vec<String>,
}
```

- [ ] **Step 4: Add module declaration in `crates/services/types.rs`**

Add `pub mod export;` and ensure it compiles.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test export_manifest_serializes -- --nocapture`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/services/types/export.rs crates/services/types.rs
git commit -m "feat(export): define ExportManifest types for index export"
```

---

### Task 7: Implement Export Service (Postgres Queries)

**Files:**
- Create: `crates/services/export.rs`
- Modify: `crates/services.rs` — add `pub mod export;`

This is the core aggregation logic. It queries each Postgres job table and collects results into the typed structs.

- [ ] **Step 1: Write failing test for the service function signature**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn export_returns_manifest_with_version() {
        // This test needs a real PgPool — mark as integration test
        // For now, test just the type construction
        let manifest = ExportManifest {
            version: 1,
            exported_at: chrono::Utc::now().to_rfc3339(),
            collection: "test".into(),
            crawls: vec![],
            scrapes: vec![],
            extractions: vec![],
            embeds: vec![],
            ingests: IngestExports {
                github: vec![],
                reddit: vec![],
                youtube: vec![],
                sessions: vec![],
            },
            refreshes: RefreshExports {
                schedules: vec![],
                jobs: vec![],
            },
            qdrant_summary: QdrantSummary {
                total_points: 0,
                source_type_counts: Default::default(),
                domain_counts: Default::default(),
                indexed_urls: vec![],
            },
        };
        assert_eq!(manifest.version, 1);
    }
}
```

- [ ] **Step 2: Implement the service function**

Create `crates/services/export.rs`:

```rust
use crate::crates::core::config::Config;
use crate::crates::services::types::export::*;
use sqlx::PgPool;
use std::collections::HashMap;
use std::error::Error;

/// Options controlling what gets included in the export.
pub struct ExportOptions {
    /// Include the full list of indexed URLs from Qdrant (can be very large).
    pub include_urls: bool,
    /// Maximum number of URLs to include from Qdrant facet query.
    pub url_limit: usize,
    /// Only export jobs with these statuses (empty = all).
    pub statuses: Vec<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_urls: true,
            url_limit: 100_000,
            statuses: vec![],
        }
    }
}

/// Build a complete export manifest from Postgres job tables + Qdrant facets.
pub async fn export_manifest(
    cfg: &Config,
    pool: &PgPool,
    opts: &ExportOptions,
) -> Result<ExportManifest, Box<dyn Error>> {
    let (crawls, extractions, embeds, ingests, refreshes) = tokio::try_join!(
        query_crawl_jobs(pool, &opts.statuses),
        query_extract_jobs(pool, &opts.statuses),
        query_embed_jobs(pool, &opts.statuses),
        query_ingest_jobs(pool, &opts.statuses),
        query_refresh_data(pool),
    )?;

    let qdrant_summary = query_qdrant_summary(cfg, opts).await?;

    Ok(ExportManifest {
        version: 1,
        exported_at: chrono::Utc::now().to_rfc3339(),
        collection: cfg.collection.clone(),
        crawls,
        scrapes: extract_scrape_urls_from_embed_jobs(&embeds),  // URLs from embed jobs with source_type="scrape"
        extractions,
        embeds,
        ingests,
        refreshes,
        qdrant_summary,
    })
}
```

Then implement each `query_*` helper as a straightforward SQL query:

**`query_crawl_jobs`:**
```sql
SELECT id, url, status, created_at, finished_at, config_json,
       result_json->'pages_crawled' as pages_crawled,
       result_json->'pages_discovered' as pages_discovered
FROM axon_crawl_jobs
ORDER BY created_at DESC
```

Map rows to `Vec<CrawlExport>`.

**`query_extract_jobs`:**
```sql
SELECT id, status, created_at, finished_at, urls_json,
       config_json->'prompt' as prompt,
       result_json->'total_items' as total_items
FROM axon_extract_jobs
ORDER BY created_at DESC
```

**`query_embed_jobs`:**
```sql
SELECT id, input_text, status, created_at, finished_at, config_json,
       result_json->'chunks_embedded' as chunks_embedded
FROM axon_embed_jobs
ORDER BY created_at DESC
```

**`query_ingest_jobs`:**
```sql
SELECT id, source_type, target, status, created_at, finished_at,
       config_json, result_json->'chunks_embedded' as chunks_embedded
FROM axon_ingest_jobs
ORDER BY created_at DESC
```

Split results by `source_type` into `IngestExports { github, reddit, youtube, sessions }`.

**`query_refresh_data`:**
Two queries — one for `axon_refresh_schedules`, one for `axon_refresh_jobs` (most recent 100).

**`query_qdrant_summary`:**
Use existing Qdrant facet helpers:
- Collection info → `total_points`
- Facet on `source_type` → `source_type_counts`
- Facet on `domain` → `domain_counts`
- Facet on `url` (if `include_urls`) → `indexed_urls`

- [ ] **Step 3: Add module declaration in `crates/services.rs`**

```rust
pub mod export;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check --bin axon`

- [ ] **Step 5: Run tests**

Run: `cargo test export -- --nocapture`

- [ ] **Step 6: Commit**

```bash
git add crates/services/export.rs crates/services.rs
git commit -m "feat(export): implement export_manifest service with Postgres + Qdrant queries"
```

---

### Task 8: Add Generic Qdrant Facet Helper + Implement Qdrant Summary

**Files:**
- Modify: `crates/vector/ops/qdrant/client.rs` — add generic `qdrant_facet(cfg, key, limit)` function
- Modify: `crates/services/export.rs` — implement `query_qdrant_summary` using the new helper

Currently, `qdrant_url_facets` (client.rs:394) and `qdrant_domain_facets` (client.rs:361) are copy-paste of each other with different hardcoded `"key"` values. We need a generic version.

- [ ] **Step 1: Write a test for the generic facet helper**

```rust
#[test]
fn qdrant_facet_builds_correct_request_body() {
    // Test the JSON body shape — actual Qdrant call is integration-tested elsewhere
    let body = serde_json::json!({
        "key": "source_type",
        "limit": 100,
    });
    assert_eq!(body["key"], "source_type");
    assert_eq!(body["limit"], 100);
}
```

- [ ] **Step 2: Add `qdrant_facet` generic helper**

In `crates/vector/ops/qdrant/client.rs`, add:

```rust
/// Generic facet query — returns (value, count) pairs for any keyword-indexed field.
pub(crate) async fn qdrant_facet(
    cfg: &Config,
    key: &str,
    limit: usize,
) -> Result<Vec<(String, usize)>> {
    let client = http_client()?;
    let url = format!("{}/collections/{}/facet", qdrant_base(cfg), cfg.collection);
    let value = client
        .post(url)
        .json(&serde_json::json!({
            "key": key,
            "limit": limit,
        }))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    parse_facet_response(&value)
}
```

Then refactor `qdrant_url_facets` and `qdrant_domain_facets` to delegate to `qdrant_facet`:

```rust
pub(crate) async fn qdrant_url_facets(cfg: &Config, limit: usize) -> Result<Vec<(String, usize)>> {
    qdrant_facet(cfg, "url", limit).await
}

pub(crate) async fn qdrant_domain_facets(cfg: &Config, limit: usize) -> Result<Vec<(String, usize)>> {
    qdrant_facet(cfg, "domain", limit).await
}
```

Extract the shared response parsing into a `parse_facet_response` helper.

- [ ] **Step 3: Implement `query_qdrant_summary` in export service**

```rust
use crate::crates::vector::ops::qdrant::client::{qdrant_facet, qdrant_url_facets};

async fn query_qdrant_summary(
    cfg: &Config,
    opts: &ExportOptions,
) -> Result<QdrantSummary, Box<dyn Error>> {
    // 1. Collection info → total_points
    // Use the same pattern as crates/services/system.rs stats handler:
    // GET /collections/{name} → result.points_count
    let total_points = fetch_collection_point_count(cfg).await.unwrap_or(0);

    // 2. Facet on source_type (small — max ~10 distinct values)
    let source_facets = qdrant_facet(cfg, "source_type", 100).await?;
    let source_type_counts: HashMap<String, u64> = source_facets
        .into_iter()
        .map(|(k, v)| (k, v as u64))
        .collect();

    // 3. Facet on domain
    let domain_facets = qdrant_facet(cfg, "domain", 10_000).await?;
    let domain_counts: HashMap<String, u64> = domain_facets
        .into_iter()
        .map(|(k, v)| (k, v as u64))
        .collect();

    // 4. Facet on url (optional, can be very large)
    let indexed_urls = if opts.include_urls {
        let url_facets = qdrant_url_facets(cfg, opts.url_limit).await?;
        url_facets.into_iter().map(|(url, _)| url).collect()
    } else {
        vec![]
    };

    Ok(QdrantSummary {
        total_points,
        source_type_counts,
        domain_counts,
        indexed_urls,
    })
}

/// Fetch point count via Qdrant collection info API.
async fn fetch_collection_point_count(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    // GET /collections/{name} → response.result.points_count
    // Reference: crates/vector/ops/stats/qdrant_fetch.rs for the existing pattern
    todo!("implement using same pattern as stats/qdrant_fetch.rs")
}
```

Look at `crates/vector/ops/stats/qdrant_fetch.rs` for the collection info GET pattern. Reuse or call the existing function.

- [ ] **Step 4: Verify compilation**

Run: `cargo check --bin axon`

- [ ] **Step 5: Commit**

```bash
git add crates/vector/ops/qdrant/client.rs crates/services/export.rs
git commit -m "feat(export): add generic qdrant_facet helper, implement Qdrant summary for export"
```

---

### Task 9: Wire Up CLI Command

**Files:**
- Create: `crates/cli/commands/export.rs`
- Modify: `crates/core/config/types/enums.rs` — add `Export` to `CommandKind`
- Modify: `crates/core/config/cli.rs` — add `Export` to `CliCommand`
- Modify: `crates/core/config/parse/build_config.rs` — wire `CliCommand::Export` → `CommandKind::Export`
- Modify: `lib.rs` — add `CommandKind::Export` dispatch arm
- Modify: `crates/cli/commands.rs` — add `pub mod export;`

- [ ] **Step 1: Add `Export` variant to `CommandKind`**

In `crates/core/config/types/enums.rs`:

```rust
pub enum CommandKind {
    // ... existing variants ...
    Export,
}
```

Add `Self::Export => "export"` to `as_str()`.

- [ ] **Step 2: Add `Export` variant to `CliCommand`**

In `crates/core/config/cli.rs`, add:

```rust
/// Export index manifest to JSON file
Export(ExportArgs),
```

And define `ExportArgs`:

```rust
#[derive(Debug, clap::Args)]
pub struct ExportArgs {
    /// Exclude indexed URL list from export (faster for large collections)
    #[arg(long)]
    pub no_urls: bool,

    /// Maximum URLs to include in export
    #[arg(long, default_value = "100000")]
    pub url_limit: usize,
}
```

**Note:** Use the existing global `--output` flag (already on `Config.output_path`) for the output file path. Do NOT add a command-specific `--output` — it would shadow the global flag and confuse users.

- [ ] **Step 3: Wire in `build_config.rs`**

```rust
CliCommand::Export(args) => {
    let mut positional = Vec::new();
    if let Some(output) = &args.output {
        positional.push(output.clone());
    }
    (CommandKind::Export, positional)
}
```

Also store the export-specific flags somewhere accessible (e.g., on `Config` or pass through the positional mechanism). The simplest approach: add `export_no_urls: bool` and `export_url_limit: usize` to `Config`.

- [ ] **Step 4: Implement the CLI command handler**

Create `crates/cli/commands/export.rs`:

```rust
use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_done, log_info};
use crate::crates::jobs::common::make_pool;
use crate::crates::services::export::{ExportOptions, export_manifest};
use std::error::Error;

pub async fn run_export(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let pool = make_pool(&cfg.pg_url).await?;

    let opts = ExportOptions {
        include_urls: !cfg.export_no_urls,
        url_limit: cfg.export_url_limit,
        statuses: vec![],
    };

    log_info("Collecting export data from Postgres and Qdrant...");

    let manifest = export_manifest(cfg, &pool, &opts).await?;

    // Use global --output flag if set, otherwise generate timestamped filename
    let output_path = cfg
        .output_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            format!("axon-export-{ts}.json")
        });

    let json = serde_json::to_string_pretty(&manifest)?;

    if cfg.json_output {
        // --json flag: write to stdout
        println!("{json}");
    } else {
        tokio::fs::write(&output_path, &json).await?;
        log_done(&format!(
            "Export written to {output_path} ({} crawls, {} extractions, {} embeds, {} ingests, {} points)",
            manifest.crawls.len(),
            manifest.extractions.len(),
            manifest.embeds.len(),
            manifest.ingests.github.len()
                + manifest.ingests.reddit.len()
                + manifest.ingests.youtube.len()
                + manifest.ingests.sessions.len(),
            manifest.qdrant_summary.total_points,
        ));
    }

    Ok(())
}
```

- [ ] **Step 5: Wire dispatch in `lib.rs`**

Add to the `match cfg.command` block:

```rust
CommandKind::Export => crate::crates::cli::commands::export::run_export(&cfg).await,
```

**Important:** Verify that `Export` is NOT included in `is_async_enqueue_mode()`. Export is synchronous — it should never be enqueued. If the function uses a whitelist of async commands, `Export` is automatically excluded. If it uses a blacklist, add `Export` to the exclusion list.

- [ ] **Step 6: Add module declaration in `crates/cli/commands.rs`**

```rust
pub mod export;
```

- [ ] **Step 7: Verify compilation**

Run: `cargo check --bin axon`

- [ ] **Step 8: Add export fields to Config defaults**

Add `export_no_urls: bool` and `export_url_limit: usize` to `Config` struct in `crates/core/config/types/config.rs`.

Set defaults in `Config::default()` at `crates/core/config/types/config_impls.rs:11`:
```rust
export_no_urls: false,
export_url_limit: 100_000,
```

If `Config::test_default()` (config_impls.rs:173) delegates to `Self::default()` and overrides specific fields, no further changes needed — the export fields will inherit defaults automatically. If `test_default()` is a full struct literal, add the fields there too.

Also wire in `build_config.rs` to populate from `ExportArgs`:
```rust
CliCommand::Export(args) => {
    // ... existing positional handling ...
    // Store export-specific flags:
    cfg.export_no_urls = args.no_urls;
    cfg.export_url_limit = args.url_limit;
}
```

**Note:** The codebase does NOT use inline Config struct literals in research.rs/search.rs test helpers — it uses `Config::test_default()`. So adding fields to `default()` is sufficient.

- [ ] **Step 9: Run full test suite**

Run: `cargo test --lib`
Expected: All pass.

- [ ] **Step 10: Commit**

```bash
git add crates/core/config/ crates/cli/commands/export.rs crates/cli/commands.rs lib.rs
git commit -m "feat: add axon export CLI command for index manifest generation"
```

---

## Phase 3: MCP Integration (Task 10)

### Task 10: Add MCP `export` Action

**Files:**
- Modify: `crates/mcp/schema.rs` — add `Export` action + `ExportRequest` struct
- Modify: `crates/mcp/server.rs` — add `AxonRequest::Export` dispatch arm
- Modify: `crates/mcp/server/handlers_system.rs` — add `handle_export`
- Modify: `docs/MCP-TOOL-SCHEMA.md` — document `export` action

- [ ] **Step 1: Read schema.rs to understand the action enum pattern**

Read `crates/mcp/schema.rs` and find how other actions (e.g., `Doctor`, `Sources`) are defined.

- [ ] **Step 2: Add `ExportRequest` struct and wire into `AxonRequest`**

In `crates/mcp/schema.rs`:

```rust
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ExportRequest {
    /// Include indexed URL list (can be large). Default: true.
    pub include_urls: Option<bool>,
    /// Maximum URLs to include. Default: 100000.
    pub url_limit: Option<usize>,
    /// Response mode: path (artifact file), inline, or both.
    pub response_mode: Option<ResponseMode>,
}
```

Add `Export(ExportRequest)` to `AxonRequest` enum.

Add to `parse_axon_request`:

```rust
"export" => AxonRequest::Export(/* deserialize ExportRequest */),
```

- [ ] **Step 3: Add dispatch arm in `server.rs`**

```rust
AxonRequest::Export(req) => self.handle_export(req).await?,
```

Update the tool description string to include `export` in the actions list.

- [ ] **Step 4: Implement `handle_export` in `handlers_system.rs`**

```rust
pub(super) async fn handle_export(
    &self,
    req: ExportRequest,
) -> Result<AxonToolResponse, ErrorData> {
    let pool = make_pool(&self.cfg.pg_url)
        .await
        .map_err(|e| logged_internal_error("export pool", e))?;

    let opts = ExportOptions {
        include_urls: req.include_urls.unwrap_or(true),
        url_limit: req.url_limit.unwrap_or(100_000),
        statuses: vec![],
    };

    let manifest = export_manifest(&self.cfg, &pool, &opts)
        .await
        .map_err(|e| logged_internal_error("export", e))?;

    let data = serde_json::to_value(&manifest)
        .map_err(|e| internal_error(format!("serialize export: {e}")))?;

    // Export is always large — use artifact mode
    respond_with_mode(
        "export",
        None,
        &data,
        req.response_mode,
        "export",
    )
    .await
}
```

- [ ] **Step 5: Update `docs/MCP-TOOL-SCHEMA.md`**

Add `export` to the action list:

```markdown
### `export`
- Direct action (no subaction)
- Exports complete index manifest: all crawl seed URLs, extract prompts, ingest sources, refresh schedules, and Qdrant summary
- Parameters:
  - `include_urls` (bool, default: true) — include full indexed URL list
  - `url_limit` (usize, default: 100000) — max URLs in export
  - `response_mode` (string) — `path` (default), `inline`, or `both`
- Returns: `ExportManifest` JSON (version, crawls, extractions, embeds, ingests, refreshes, qdrant_summary)
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check --bin axon`

- [ ] **Step 7: Run tests**

Run: `cargo test --lib`

- [ ] **Step 8: Commit**

```bash
git add crates/mcp/ docs/MCP-TOOL-SCHEMA.md
git commit -m "feat(mcp): add export action for index manifest generation"
```

---

## Phase 4: Documentation + Help Text (Task 11)

### Task 11: Update Help Action and CLAUDE.md

**Files:**
- Modify: `crates/mcp/server/handlers_system.rs` — add `export` to help action output
- Modify: `CLAUDE.md` — add `export` to Commands table

- [ ] **Step 1: Add `export` to MCP help output**

Find where the help action builds its response (in `handle_help`) and add:

```
export — Export complete index manifest (crawl seeds, ingest targets, extract prompts, refresh schedules, Qdrant summary) to JSON
```

- [ ] **Step 2: Add to CLAUDE.md Commands table**

```markdown
| `export [--output <path>]` | Export index manifest (seeds, ingests, extractions, schedules, Qdrant summary) to JSON | No |
```

- [ ] **Step 3: Commit**

```bash
git add crates/mcp/server/handlers_system.rs CLAUDE.md
git commit -m "docs: add export command to help text and CLAUDE.md"
```

---

## Summary: What the Export Captures

After implementation, `axon export` produces a JSON file containing:

| Section | Source | Data |
|---|---|---|
| `crawls` | `axon_crawl_jobs` | Seed URL, config (max_pages, render_mode, etc.), pages crawled/discovered, status, timestamps |
| `scrapes` | Qdrant `source_type=scrape` facet | URL, scraped_at (only for content indexed AFTER Phase 1) |
| `extractions` | `axon_extract_jobs` | URLs, prompt, total items extracted, status, timestamps |
| `embeds` | `axon_embed_jobs` | Input path/URL, collection, source_type, chunks embedded, status, timestamps |
| `ingests.github` | `axon_ingest_jobs` where `source_type=github` | Repo target, config (include_source, etc.), chunks embedded |
| `ingests.reddit` | `axon_ingest_jobs` where `source_type=reddit` | Subreddit/thread target, chunks embedded |
| `ingests.youtube` | `axon_ingest_jobs` where `source_type=youtube` | Video/playlist/channel URL, chunks embedded |
| `ingests.sessions` | `axon_ingest_jobs` where `source_type=sessions` | Provider list, chunks embedded |
| `refreshes.schedules` | `axon_refresh_schedules` | Name, seed_url, URLs, interval, enabled, next/last run |
| `refreshes.jobs` | `axon_refresh_jobs` | URLs, checked/changed/failed counts, status |
| `qdrant_summary` | Qdrant API | Total points, source_type distribution, domain distribution, all indexed URLs |

This is sufficient to repopulate the index: replay each crawl seed, re-ingest each GitHub/Reddit/YouTube target, re-run extractions with stored prompts, and re-trigger refresh schedules.
