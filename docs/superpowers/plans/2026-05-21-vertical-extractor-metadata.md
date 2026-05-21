# Vertical Extractor Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `extra: Option<serde_json::Value>` to `ScrapedDoc` and `ScrapeResult`, fix the scrape→embed path so vertical metadata reaches Qdrant instead of being discarded at the disk-write step, and implement per-extractor `build_extra()` functions for all 17 in-scope vertical extractors so their curated metadata fields land as flat Qdrant payload keys.

**Architecture:** Each vertical extractor gains a `build_extra()` pure function that produces a `serde_json::Value::Object` of prefixed payload fields (`pkg_*`, `hf_*`, `git_*`, etc.). `ScrapedDoc` gains the `extra` field to carry this data from extractor → service → CLI. The scrape CLI path is restructured: instead of writing markdown to disk and losing everything except `(url, markdown)`, `scrape_one()` returns a `PreparedDoc` built directly from the `ScrapeResult`, preserving `extra`, `extractor_name`, and `title`. The `PreparedDoc.extra` merge already works in `pipeline.rs:122-126` — the only broken bridge is the CLI hand-off. Payload indexes for the new vertical fields are added to `payload_indexes.rs`.

**Tech Stack:** Rust, `serde_json`, Qdrant REST API (`PUT /collections/{name}/index`), existing `PreparedDoc` + `embed_prepared_docs` pipeline (unchanged). All code in the single `axon` crate.

---

## Extractor Inventory

| Extractor | Prefix | New indexed fields | Needs test sidecar? | Line count |
|-----------|--------|--------------------|---------------------|------------|
| `github_repo` | `git_` | (via git_payload.rs) | no (no tests yet, add one) | 289 |
| `github_issue` | `git_` | (via git_payload.rs) | has `github_issue_tests.rs` | 212 |
| `github_pr` | `git_` | (via git_payload.rs) | has `github_pr_tests.rs` | 222 |
| `github_release` | `git_` | (via git_payload.rs) | no (add one) | 199 |
| `npm` | `pkg_*` | `pkg_registry`, `pkg_name`, `pkg_language`, `pkg_license`, `pkg_author` | no (add one) | 249 |
| `pypi` | `pkg_*` | `pkg_registry`, `pkg_name`, `pkg_language`, `pkg_license`, `pkg_author` | no (add one) | 243 |
| `crates_io` | `pkg_*` + `crate_*` | `pkg_registry`, `pkg_name`, `pkg_language`, `pkg_license` | has `crates_io_tests.rs` | 372 |
| `docs_rs` | `pkg_*` + `docrs_*` | `pkg_registry`, `pkg_name`, `pkg_language` | has `docs_rs_tests.rs` | 282 |
| `docker_hub` | `docker_*` | none (stored only) | no (add one) | 147 |
| `huggingface_model` | `hf_*` | `hf_task`, `hf_library` | no (add one) | 221 |
| `dev_to` | `devto_*` | `devto_author` | no (add one) | 163 |
| `shopify` | `shop_*` | none (stored only) | no (add one) | 213 |
| `hackernews` | `hn_*` | `hn_type`, `hn_author` | has `hackernews_tests.rs` | 239 |
| `stackoverflow` | `so_*` | `so_question_id`, `so_is_answered` | has `stackoverflow_tests.rs` | 246 |
| `arxiv` | `arxiv_*` | `arxiv_id` | has `arxiv_tests.rs` | 232 |
| `amazon` | `amz_*` | none (stored only) | no (add one) | 286 |
| `ebay` | `ebay_*` | none (stored only) | no (add one) | 289 |

**Out of scope:** `reddit` — the spec explicitly excludes it. The reddit vertical stores a `structured_blob` but does NOT emit flat `reddit_*` fields (those come only from the ingest path, not the scrape vertical). No `youtube_video.rs` exists in the verticals directory.

---

## Files Modified or Created

### Modified
- `src/extract/types.rs` — add `extra: Option<serde_json::Value>` to `ScrapedDoc`
- `src/services/types/service.rs` — add `extra` and `extractor_name` fields to `ScrapeResult`
- `src/services/scrape.rs` — thread `doc.extra`, `doc.title`, `doc.extractor_name` into `ScrapeResult` from vertical dispatch
- `src/cli/commands/scrape.rs` — change `scrape_one()` return to `Option<PreparedDoc>`, call `embed_prepared_docs` directly instead of disk-write path
- `src/vector/ops/tei/qdrant_store/payload_indexes.rs` — add vertical keyword and integer indexes
- `src/extract/verticals/npm.rs` — add `build_extra()`; populate `extra` in `ScrapedDoc`
- `src/extract/verticals/pypi.rs` — add `build_extra()`; populate `extra` in `ScrapedDoc`
- `src/extract/verticals/crates_io.rs` — add `build_extra()`; populate `extra` in `ScrapedDoc`
- `src/extract/verticals/docs_rs.rs` — add `build_extra()` + refactor to expose item_count to `extract()`; populate `extra`
- `src/extract/verticals/docker_hub.rs` — add `build_extra()`; populate `extra`
- `src/extract/verticals/huggingface_model.rs` — add `build_extra()`; populate `extra`
- `src/extract/verticals/dev_to.rs` — add `build_extra()`; populate `extra`
- `src/extract/verticals/shopify.rs` — add `build_extra()`; populate `extra`
- `src/extract/verticals/hackernews.rs` — add `build_extra()`; populate `extra` (also bump `extractor_version` to 2)
- `src/extract/verticals/stackoverflow.rs` — add `build_extra()`; populate `extra` (also bump `extractor_version` to 2)
- `src/extract/verticals/arxiv.rs` — add `build_extra()`; populate `extra` (also bump `extractor_version` to 2)
- `src/extract/verticals/amazon.rs` — add `build_extra()`; populate `extra`
- `src/extract/verticals/ebay.rs` — add `build_extra()`; populate `extra`
- `src/extract/verticals/github_repo.rs` — add `build_extra()` using `build_git_payload`; populate `extra`
- `src/extract/verticals/github_issue.rs` — add `build_extra()` using `build_git_payload`; populate `extra` (bump version to 2)
- `src/extract/verticals/github_pr.rs` — add `build_extra()` using `build_git_payload`; populate `extra` (bump version to 2)
- `src/extract/verticals/github_release.rs` — add `build_extra()` using `build_git_payload`; populate `extra` (bump version to 2)
- `docs/specs/vertical-extractor-metadata.md` — update Implementation Status table from "pending" to "done" for each item as it ships

### Created (new test sidecars)
- `src/extract/verticals/npm_tests.rs`
- `src/extract/verticals/pypi_tests.rs`
- `src/extract/verticals/docker_hub_tests.rs`
- `src/extract/verticals/huggingface_model_tests.rs`
- `src/extract/verticals/dev_to_tests.rs`
- `src/extract/verticals/shopify_tests.rs`
- `src/extract/verticals/amazon_tests.rs`
- `src/extract/verticals/ebay_tests.rs`
- `src/extract/verticals/github_repo_tests.rs`
- `src/extract/verticals/github_release_tests.rs`

---

## Task 1: Add `extra` to `ScrapedDoc` and `ScrapeResult`

**Files:**
- Modify: `src/extract/types.rs`
- Modify: `src/services/types/service.rs`

This is a pure type change. Every extractor returns a `ScrapedDoc` — adding the field with a default of `None` means every existing callsite compiles without change; extractors will start setting `extra` in subsequent tasks.

- [ ] **Step 1: Add `extra` field to `ScrapedDoc`**

In `src/extract/types.rs`, change the struct:

```rust
#[derive(Debug, Clone)]
pub struct ScrapedDoc {
    pub url: String,
    pub markdown: String,
    pub title: Option<String>,
    pub extractor_name: &'static str,
    pub extractor_version: u32,
    pub structured: Option<serde_json::Value>,
    pub follow_crawl_urls: Vec<String>,
    /// Curated per-extractor metadata fields to merge flat into the Qdrant payload.
    /// Every key in this object becomes a top-level payload field when the doc is embedded.
    /// Keys must follow the prefix convention: `pkg_*`, `git_*`, `hf_*`, `docker_*`, etc.
    /// Absent beats null — only set keys that have actual values.
    pub extra: Option<serde_json::Value>,
}
```

- [ ] **Step 2: Add `extra` and `extractor_name` to `ScrapeResult`**

In `src/services/types/service.rs`, find the `ScrapeResult` struct (around line 774) and add two fields:

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScrapeResult {
    pub payload: serde_json::Value,
    pub url: String,
    pub markdown: String,
    pub output: String,
    pub artifact_handle: Option<ArtifactHandle>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_estimate: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_tokens_estimate: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<DocumentBackend>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub follow_crawl_urls: Vec<String>,
    /// Curated per-extractor metadata (from `ScrapedDoc.extra`). None for generic scrapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
    /// Vertical extractor name (from `ScrapedDoc.extractor_name`). None for generic scrapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extractor_name: Option<String>,
    /// Page title from the vertical extractor. None for generic scrapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}
```

- [ ] **Step 3: Verify compile**

```bash
cargo check 2>&1 | grep "error\[" | head -20
```

Expected: errors only where `ScrapeResult` is constructed without the new optional fields (those will have `Default` from `None`). The `map_scrape_payload` function in `scrape.rs` constructs `ScrapeResult` — it needs `extra: None, extractor_name: None, title: None` added to its literal. Fix that now:

In `src/services/scrape.rs`, find `map_scrape_payload` and update the `ScrapeResult` literal:

```rust
Ok(ScrapeResult {
    payload,
    url,
    markdown,
    output,
    artifact_handle: None,
    truncated: false,
    token_estimate: None,
    next_cursor: None,
    remaining_tokens_estimate: None,
    backend: Some(DocumentBackend::LiveScrape),
    follow_crawl_urls: vec![],
    extra: None,
    extractor_name: None,
    title: None,
})
```

- [ ] **Step 4: Verify compile again**

```bash
cargo check 2>&1 | grep "error\[" | head -20
```

Expected: 0 errors.

- [ ] **Step 5: Commit**

```bash
git add src/extract/types.rs src/services/types/service.rs src/services/scrape.rs
git commit -m "feat(types): add extra/extractor_name/title fields to ScrapedDoc and ScrapeResult"
```

---

## Task 2: Thread vertical metadata through `services/scrape.rs`

**Files:**
- Modify: `src/services/scrape.rs` lines 89–100

Currently the vertical dispatch path (line 91) throws away `doc.extra`, `doc.title`, and `doc.extractor_name`:

```rust
// BEFORE (discards metadata):
let payload = serde_json::json!({ "url": doc.url, "markdown": doc.markdown });
let mut scrape_result = map_scrape_payload(payload)?;
scrape_result.backend = Some(DocumentBackend::LiveScrape);
scrape_result.follow_crawl_urls = doc.follow_crawl_urls;
```

- [ ] **Step 1: Update the vertical dispatch block to preserve metadata**

Replace lines 89–100 in `src/services/scrape.rs` with:

```rust
Ok(Some(result)) => {
    let doc = result.map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;
    let payload = serde_json::json!({ "url": doc.url, "markdown": doc.markdown });
    let mut scrape_result = map_scrape_payload(payload)?;
    scrape_result.backend = Some(DocumentBackend::LiveScrape);
    scrape_result.follow_crawl_urls = doc.follow_crawl_urls;
    // Preserve vertical extractor metadata — these fields flow through to PreparedDoc
    // and ultimately land as flat Qdrant payload keys. Without this, scrape→embed
    // loses all curated metadata even though the extractor produced it.
    scrape_result.extra = doc.extra;
    scrape_result.extractor_name = Some(doc.extractor_name.to_string());
    scrape_result.title = doc.title;
    tracing::debug!(
        url = %normalized,
        extractor = doc.extractor_name,
        has_extra = scrape_result.extra.is_some(),
        "vertical.dispatched: extractor handled scrape"
    );
    return Ok(scrape_result);
}
```

- [ ] **Step 2: Verify compile**

```bash
cargo check 2>&1 | grep "error\[" | head -5
```

Expected: 0 errors.

- [ ] **Step 3: Commit**

```bash
git add src/services/scrape.rs
git commit -m "fix(scrape): preserve extra/extractor_name/title from vertical dispatch in ScrapeResult"
```

---

## Task 3: Fix the scrape CLI embed path to use `PreparedDoc` directly

**Files:**
- Modify: `src/cli/commands/scrape.rs`

Currently `scrape_one()` returns `Option<(String, String)>` — a `(url, markdown)` pair — which is then written to a temp directory and re-read by `embed_now_with_source`. This destroys all vertical metadata. The fix changes `scrape_one()` to return `Option<PreparedDoc>` and batch-embeds directly.

`embed_prepared_docs` is `pub(crate)` in `src/vector/ops/tei/text_embed.rs`. Since both the CLI and that module are in the same `axon` crate (single `Cargo.toml`), `pub(crate)` is accessible.

- [ ] **Step 1: Write a failing test for the new embed path**

Create `src/cli/commands/scrape/scrape_migration_tests.rs` — check if it already exists first:

```bash
ls src/cli/commands/scrape/
```

The file `scrape_migration_tests.rs` already exists (referenced in `scrape.rs` line 2). Open it and add a new test that verifies `scrape_one` builds a `PreparedDoc` with `extra` populated. Since `scrape_one` calls a live network, we test the pure conversion logic instead — verify the `ScrapeResult → PreparedDoc` mapping function works correctly:

In `src/cli/commands/scrape/scrape_migration_tests.rs`, add:

```rust
#[test]
fn scrape_result_to_prepared_doc_preserves_extra() {
    use crate::services::types::{DocumentBackend, ScrapeResult};
    use crate::vector::ops::tei::PreparedDoc;

    let extra = serde_json::json!({ "pkg_registry": "npm", "pkg_name": "lodash" });
    let result = ScrapeResult {
        payload: serde_json::json!({"url": "https://npmjs.com/package/lodash", "markdown": "# lodash"}),
        url: "https://npmjs.com/package/lodash".to_string(),
        markdown: "# lodash\n\nA utility library.".to_string(),
        output: "# lodash\n\nA utility library.".to_string(),
        artifact_handle: None,
        truncated: false,
        token_estimate: None,
        next_cursor: None,
        remaining_tokens_estimate: None,
        backend: Some(DocumentBackend::LiveScrape),
        follow_crawl_urls: vec![],
        extra: Some(extra.clone()),
        extractor_name: Some("npm".to_string()),
        title: Some("lodash@4.17.21".to_string()),
    };

    let doc = scrape_result_to_prepared_doc(&result);

    assert!(doc.extra.is_some(), "extra must be preserved");
    let doc_extra = doc.extra.unwrap();
    assert_eq!(doc_extra["pkg_registry"], "npm");
    assert_eq!(doc_extra["pkg_name"], "lodash");
    assert_eq!(doc.extractor_name, Some("npm".to_string()));
    assert_eq!(doc.title, Some("lodash@4.17.21".to_string()));
    assert_eq!(doc.source_type, "scrape");
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test scrape_result_to_prepared_doc_preserves_extra 2>&1 | tail -20
```

Expected: compile error — `scrape_result_to_prepared_doc` not defined yet.

- [ ] **Step 3: Rewrite `run_scrape` and `scrape_one` in `src/cli/commands/scrape.rs`**

Replace the current `run_scrape` and `scrape_one` functions completely (keep `print_scrape_preamble`, `emit_scrape_result`, and `run_explicit_vertical` unchanged):

```rust
use crate::vector::ops::input::chunk_markdown;
use crate::vector::ops::tei::{PreparedDoc, embed_prepared_docs};
use spider::url::Url as SpiderUrl;

/// Convert a `ScrapeResult` into a `PreparedDoc` suitable for direct embedding.
///
/// Preserves `extra`, `extractor_name`, and `title` from the vertical extractor —
/// these would be discarded if we went through the disk-write path instead.
pub(crate) fn scrape_result_to_prepared_doc(result: &crate::services::types::ScrapeResult) -> PreparedDoc {
    let domain = SpiderUrl::parse(&result.url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    let chunks = chunk_markdown(&result.markdown);
    PreparedDoc {
        url: result.url.clone(),
        domain,
        chunks,
        source_type: "scrape".to_string(),
        content_type: "markdown",
        title: result.title.clone(),
        extra: result.extra.clone(),
        extractor_name: result.extractor_name.clone(),
        structured: None,
    }
}

pub async fn run_scrape(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if let Ok(name) = std::env::var("AXON_VERTICAL")
        && !name.is_empty()
    {
        return run_explicit_vertical(cfg, &name).await;
    }

    let urls = parse_urls(cfg);
    if urls.is_empty() {
        return Err(
            anyhow::anyhow!("scrape requires at least one URL (positional or --urls)").into(),
        );
    }
    if cfg.output_path.is_some() && urls.len() > 1 {
        return Err(anyhow::anyhow!(
            "--output cannot be used with multiple URLs (each would overwrite the same file)"
        )
        .into());
    }
    log_info(&format!(
        "command=scrape urls={} format={:?} wait={}",
        urls.len(),
        cfg.format,
        cfg.wait
    ));

    // Phase 1: scrape URLs concurrently, bounded by batch_concurrency.
    let concurrency = cfg.batch_concurrency.max(1);
    let mut to_embed: Vec<PreparedDoc> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let results: Vec<_> = stream::iter(&urls)
        .map(|url| scrape_one(cfg, url))
        .buffer_unordered(concurrency)
        .collect()
        .await;
    for result in results {
        match result {
            Ok(Some(doc)) => to_embed.push(doc),
            Ok(None) => {}
            Err(e) => {
                log_warn(&format!("scrape error={e}"));
                errors.push(e.to_string());
            }
        }
    }

    // Phase 2: embed all collected PreparedDocs directly — no disk write, no metadata loss.
    if cfg.embed && !to_embed.is_empty() {
        embed_prepared_docs(cfg, to_embed, None).await.map_err(|e| -> Box<dyn Error> {
            format!("embed failed: {e}").into()
        })?;
    }

    if !errors.is_empty() {
        return Err(format!(
            "{} scrape(s) failed:\n  {}",
            errors.len(),
            errors.join("\n  ")
        )
        .into());
    }

    Ok(())
}

/// Scrapes one URL and returns `Some(PreparedDoc)` when `cfg.embed` is true,
/// `None` otherwise. Metadata from vertical extractors (extra, title,
/// extractor_name) is preserved in the returned PreparedDoc.
async fn scrape_one(cfg: &Config, url: &str) -> Result<Option<PreparedDoc>, Box<dyn Error>> {
    print_scrape_preamble(cfg, url);

    validate_url(url)?;
    let result = scrape_service::scrape(cfg, url, None).await?;
    let follow_crawl_urls = result.follow_crawl_urls.clone();
    let normalized = result.url.clone();

    emit_scrape_result(cfg, &result)?;

    // Enqueue follow-up crawl jobs (e.g. docs.rs crawl after crates.io scrape).
    if cfg.embed && !follow_crawl_urls.is_empty() {
        let unique: Vec<&String> = follow_crawl_urls
            .iter()
            .filter(|u| u.as_str() != normalized.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .take(5)
            .collect();
        for follow_url in unique {
            match crate::jobs::crawl::start_crawl_job(cfg, follow_url).await {
                Ok(job_id) => log_info(&format!(
                    "queued follow-up crawl: url={follow_url} job={job_id}"
                )),
                Err(e) => log_warn(&format!(
                    "could not queue follow-up crawl: url={follow_url} err={e}"
                )),
            }
        }
    }

    if cfg.embed {
        Ok(Some(scrape_result_to_prepared_doc(&result)))
    } else {
        Ok(None)
    }
}
```

Remove the unused `Uuid` import (it was only needed for `run_id`) and the `embed_service` import (replaced by `embed_prepared_docs`). Update imports at the top of `scrape.rs`:

```rust
// Remove these:
// use crate::core::content::url_to_filename;
// use crate::services::embed as embed_service;
// use uuid::Uuid;

// Add these (if not already present):
use crate::vector::ops::input::chunk_markdown;
use crate::vector::ops::tei::{PreparedDoc, embed_prepared_docs};
use spider::url::Url as SpiderUrl;
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cargo test scrape_result_to_prepared_doc_preserves_extra 2>&1 | tail -10
```

Expected: `test scrape_result_to_prepared_doc_preserves_extra ... ok`

- [ ] **Step 5: Verify the whole crate compiles**

```bash
cargo check 2>&1 | grep "error\[" | head -20
```

Expected: 0 errors. If there are unused import warnings, clean them up.

- [ ] **Step 6: Run the full test suite**

```bash
cargo test 2>&1 | tail -30
```

Expected: all tests pass. If any tests reference the old `(String, String)` return type of `scrape_one`, update them to match the new `Option<PreparedDoc>` return type.

- [ ] **Step 7: Commit**

```bash
git add src/cli/commands/scrape.rs src/cli/commands/scrape/scrape_migration_tests.rs
git commit -m "fix(scrape): replace disk-write embed path with direct PreparedDoc embed — preserves vertical metadata"
```

---

## Task 4: GitHub vertical extractors — add `build_extra()` using `build_git_payload`

**Files:**
- Modify: `src/extract/verticals/github_repo.rs`
- Modify: `src/extract/verticals/github_issue.rs`
- Modify: `src/extract/verticals/github_pr.rs`
- Modify: `src/extract/verticals/github_release.rs`
- Create: `src/extract/verticals/github_repo_tests.rs`
- Create: `src/extract/verticals/github_release_tests.rs`

All four GitHub verticals use the shared `git_payload.rs` builder. The `build_git_payload()` function returns a `serde_json::Value::Object` that becomes `extra` directly. This is the cleanest approach — the `git_*` fields from `build_git_payload()` match the spec exactly.

- [ ] **Step 1: Add `build_extra` to `github_repo.rs`**

In `src/extract/verticals/github_repo.rs`, add this import at the top:

```rust
use crate::ingest::git_payload::{GitPayload, build_git_payload};
```

Add this function before `extract()`:

```rust
fn build_extra(owner: &str, repo: &str, data: &serde_json::Value) -> serde_json::Value {
    let stars = data["stargazers_count"].as_u64();
    let forks = data["forks_count"].as_u64();
    let language = data["language"].as_str();
    let topics: Vec<String> = data["topics"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let visibility = data["visibility"].as_str();
    let clone_url = data["clone_url"].as_str();

    let mut extra = build_git_payload(&GitPayload {
        provider: "github",
        host: "github.com".to_string(),
        owner: Some(owner.to_string()),
        repo: repo.to_string(),
        content_kind: "repo_metadata",
        ..Default::default()
    });

    // Extend with repo-specific fields in git_meta
    if let Some(obj) = extra.as_object_mut() {
        obj.insert("git_meta".to_string(), serde_json::json!({
            "stars": stars,
            "forks": forks,
            "language": language,
            "topics": topics,
            "visibility": visibility,
            "clone_url": clone_url,
        }));
    }
    extra
}
```

In the `build_scraped_doc` body (the big function that returns `ScrapedDoc`), find the closing `Ok(ScrapedDoc { ... })` and add the `extra` field. You need `owner` and `repo` in scope — they are in `extract()` but not in `build_scraped_doc`. The simplest approach: compute `extra` in `extract()` after `data` is available and pass it in, or call `build_extra` inside `build_scraped_doc` by threading `owner`/`repo` parameters.

Since `build_scraped_doc` is a `fn` (not `async`), add `owner: &str, repo: &str` parameters to it and call `build_extra` from within:

Find the function signature:
```rust
// It doesn't exist as a separate fn — extract() is the single function.
// The ScrapedDoc is built inline near line 280. Add the extra field there.
```

Actually `github_repo.rs` builds the `ScrapedDoc` directly in `extract()`. Find the `Ok(ScrapedDoc { ... })` at line ~280 and add:

```rust
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title,
    extractor_name: INFO.name,
    extractor_version: 2,
    structured: Some(data.clone()),
    follow_crawl_urls,
    extra: Some(build_extra(owner, repo, &data)),
})
```

Note: `data` is moved into `structured: Some(data)`. Use `data.clone()` for `build_extra` before the move, OR pass the reference to `build_extra` before `data` is consumed. Restructure:

```rust
let extra = build_extra(owner, repo, &data);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title,
    extractor_name: INFO.name,
    extractor_version: 2,
    structured: Some(data),
    follow_crawl_urls,
    extra: Some(extra),
})
```

- [ ] **Step 2: Create test sidecar for `github_repo`**

Create `src/extract/verticals/github_repo_tests.rs`:

```rust
use super::build_extra;

#[test]
fn build_extra_sets_git_fields() {
    let data = serde_json::json!({
        "stargazers_count": 1234u64,
        "forks_count": 56u64,
        "language": "Rust",
        "topics": ["cli", "rag"],
        "visibility": "public",
        "clone_url": "https://github.com/owner/repo.git",
    });

    let extra = build_extra("owner", "repo", &data);

    assert_eq!(extra["provider"], "github");
    assert_eq!(extra["git_host"], "github.com");
    assert_eq!(extra["git_owner"], "owner");
    assert_eq!(extra["git_repo"], "repo");
    assert_eq!(extra["git_content_kind"], "repo_metadata");
    assert!(extra["git_meta"]["stars"].as_u64() == Some(1234));
}
```

Add to the end of `github_repo.rs`:

```rust
#[cfg(test)]
#[path = "github_repo_tests.rs"]
mod tests;
```

- [ ] **Step 3: Add `build_extra` to `github_issue.rs`**

In `src/extract/verticals/github_issue.rs`, add import:

```rust
use crate::ingest::git_payload::{GitPayload, build_git_payload};
```

Add function (before `build_scraped_doc`):

```rust
fn build_extra(
    owner: &str,
    repo: &str,
    number: u64,
    state: &str,
    author: &str,
    labels: &[&str],
    created_at: &str,
) -> serde_json::Value {
    build_git_payload(&GitPayload {
        provider: "github",
        host: "github.com".to_string(),
        owner: Some(owner.to_string()),
        repo: repo.to_string(),
        content_kind: "issue",
        state: if state.is_empty() { None } else { Some(state.to_string()) },
        number: Some(number),
        author: if author.is_empty() { None } else { Some(author.to_string()) },
        labels: labels.iter().map(|s| s.to_string()).collect(),
        created_at: if created_at.is_empty() { None } else { Some(created_at.to_string()) },
        ..Default::default()
    })
}
```

In `build_scraped_doc`, add the `extra` field before the `Ok(ScrapedDoc { ... })`:

```rust
let extra = build_extra(owner, repo, number, state, author, &labels, created_at);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title: Some(title),
    extractor_name: INFO.name,
    extractor_version: 2,   // bump from 1
    structured: Some(structured),
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

The existing `github_issue_tests.rs` sidecar covers `build_scraped_doc` already. Add a test for `build_extra` to it:

In `src/extract/verticals/github_issue_tests.rs`, add:

```rust
#[test]
fn build_extra_sets_git_issue_fields() {
    let extra = super::build_extra(
        "rust-lang", "rust", 12345, "open", "ferris",
        &["bug", "P-high"], "2024-01-15T10:00:00Z",
    );
    assert_eq!(extra["provider"], "github");
    assert_eq!(extra["git_content_kind"], "issue");
    assert_eq!(extra["git_state"], "open");
    assert_eq!(extra["git_number"], 12345u64);
    assert_eq!(extra["git_author"], "ferris");
}
```

- [ ] **Step 4: Add `build_extra` to `github_pr.rs`**

In `src/extract/verticals/github_pr.rs`, add import:

```rust
use crate::ingest::git_payload::{GitPayload, build_git_payload};
```

Add function:

```rust
fn build_extra(
    owner: &str,
    repo: &str,
    number: u64,
    state: &str,
    author: &str,
    labels: &[&str],
    is_draft: bool,
    merged_at: &str,
    created_at: &str,
) -> serde_json::Value {
    let pr_state = if state == "closed" && !merged_at.is_empty() {
        "merged"
    } else {
        state
    };
    build_git_payload(&GitPayload {
        provider: "github",
        host: "github.com".to_string(),
        owner: Some(owner.to_string()),
        repo: repo.to_string(),
        content_kind: "pr",
        state: if pr_state.is_empty() { None } else { Some(pr_state.to_string()) },
        number: Some(number),
        author: if author.is_empty() { None } else { Some(author.to_string()) },
        labels: labels.iter().map(|s| s.to_string()).collect(),
        is_draft: Some(is_draft),
        merged_at: if merged_at.is_empty() { None } else { Some(merged_at.to_string()) },
        created_at: if created_at.is_empty() { None } else { Some(created_at.to_string()) },
        ..Default::default()
    })
}
```

In `build_scraped_doc`, call `build_extra` and add to `ScrapedDoc`:

```rust
let extra = build_extra(owner, repo, number, state, author, &labels, draft, merged_at, created_at);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title: Some(title),
    extractor_name: INFO.name,
    extractor_version: 2,  // bump from 1
    structured: Some(structured),
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

The existing `github_pr_tests.rs` already tests `build_scraped_doc`. Add a test to it:

```rust
#[test]
fn build_extra_sets_pr_state_merged() {
    let extra = super::build_extra(
        "owner", "repo", 42, "closed", "dev",
        &[], false, "2024-03-01T12:00:00Z", "2024-01-01T12:00:00Z",
    );
    assert_eq!(extra["git_content_kind"], "pr");
    assert_eq!(extra["git_state"], "merged");
    assert_eq!(extra["git_is_draft"], false);
    assert_eq!(extra["git_merged_at"], "2024-03-01T12:00:00Z");
}
```

- [ ] **Step 5: Add `build_extra` to `github_release.rs` and test sidecar**

In `src/extract/verticals/github_release.rs`, add import:

```rust
use crate::ingest::git_payload::{GitPayload, build_git_payload};
```

Add function:

```rust
fn build_extra(owner: &str, repo: &str) -> serde_json::Value {
    build_git_payload(&GitPayload {
        provider: "github",
        host: "github.com".to_string(),
        owner: Some(owner.to_string()),
        repo: repo.to_string(),
        content_kind: "release",
        ..Default::default()
    })
}
```

In `extract()`, after `data` is available, compute and add `extra`:

```rust
let extra = build_extra(owner, repo);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title,
    extractor_name: INFO.name,
    extractor_version: 2,  // bump from 1
    structured: Some(data),
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

Create `src/extract/verticals/github_release_tests.rs`:

```rust
use super::build_extra;

#[test]
fn build_extra_sets_release_fields() {
    let extra = build_extra("tokio-rs", "tokio");
    assert_eq!(extra["provider"], "github");
    assert_eq!(extra["git_host"], "github.com");
    assert_eq!(extra["git_owner"], "tokio-rs");
    assert_eq!(extra["git_repo"], "tokio");
    assert_eq!(extra["git_content_kind"], "release");
}
```

Add to `github_release.rs`:

```rust
#[cfg(test)]
#[path = "github_release_tests.rs"]
mod tests;
```

- [ ] **Step 6: Run tests**

```bash
cargo test github_repo_tests github_issue_tests github_pr_tests github_release_tests 2>&1 | tail -20
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add src/extract/verticals/github_repo.rs \
        src/extract/verticals/github_repo_tests.rs \
        src/extract/verticals/github_issue.rs \
        src/extract/verticals/github_issue_tests.rs \
        src/extract/verticals/github_pr.rs \
        src/extract/verticals/github_pr_tests.rs \
        src/extract/verticals/github_release.rs \
        src/extract/verticals/github_release_tests.rs
git commit -m "feat(verticals): add git_* extra payload to github_repo/issue/pr/release"
```

---

## Task 5: Package registry extractors — npm, pypi, crates_io

**Files:**
- Modify: `src/extract/verticals/npm.rs`
- Modify: `src/extract/verticals/pypi.rs`
- Modify: `src/extract/verticals/crates_io.rs`
- Create: `src/extract/verticals/npm_tests.rs`
- Create: `src/extract/verticals/pypi_tests.rs`
- Modify: `src/extract/verticals/crates_io_tests.rs`

All three share the `pkg_*` prefix schema. They differ only in `pkg_registry`, `pkg_language`, and a few registry-specific fields.

- [ ] **Step 1: Add `build_extra` to `npm.rs`**

In `src/extract/verticals/npm.rs`, add this function before `extract()`. The variables `name`, `latest_version`, `license`, `author`, `keywords`, `homepage`, and `repo_url` are all in scope in `extract()` when `ScrapedDoc` is constructed:

```rust
fn build_extra(
    name: &str,
    version: &str,
    license: &str,
    author: &str,
    keywords: &[&str],
    homepage: &str,
    repo_url: Option<&str>,
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "pkg_registry": "npm",
        "pkg_name": name,
        "pkg_version": version,
        "pkg_language": "javascript",
    });
    if !license.is_empty() {
        obj["pkg_license"] = serde_json::Value::String(license.to_string());
    }
    if !author.is_empty() {
        obj["pkg_author"] = serde_json::Value::String(author.to_string());
    }
    if !keywords.is_empty() {
        obj["pkg_keywords"] = serde_json::json!(keywords);
    }
    if !homepage.is_empty() {
        obj["pkg_homepage"] = serde_json::Value::String(homepage.to_string());
    }
    if let Some(r) = repo_url {
        obj["pkg_repo_url"] = serde_json::Value::String(r.to_string());
    }
    obj
}
```

In `extract()`, compute `extra` before building `ScrapedDoc`:

```rust
let extra = build_extra(name, latest_version, license, &author, &keywords, homepage, repo_url.as_deref());
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title,
    extractor_name: INFO.name,
    extractor_version: 2,
    structured: Some(data),
    follow_crawl_urls,
    extra: Some(extra),
})
```

- [ ] **Step 2: Create `npm_tests.rs`**

Create `src/extract/verticals/npm_tests.rs`:

```rust
use super::build_extra;

#[test]
fn build_extra_minimal() {
    let extra = build_extra("lodash", "4.17.21", "MIT", "John-David Dalton", &["array", "util"], "https://lodash.com", Some("https://github.com/lodash/lodash"));
    assert_eq!(extra["pkg_registry"], "npm");
    assert_eq!(extra["pkg_name"], "lodash");
    assert_eq!(extra["pkg_version"], "4.17.21");
    assert_eq!(extra["pkg_language"], "javascript");
    assert_eq!(extra["pkg_license"], "MIT");
    assert_eq!(extra["pkg_author"], "John-David Dalton");
    assert_eq!(extra["pkg_repo_url"], "https://github.com/lodash/lodash");
}

#[test]
fn build_extra_omits_empty_fields() {
    let extra = build_extra("tiny", "1.0.0", "", "", &[], "", None);
    assert!(extra.get("pkg_license").is_none() || extra["pkg_license"].is_null());
    assert!(extra.get("pkg_author").is_none() || extra["pkg_author"].is_null());
    assert!(extra.get("pkg_homepage").is_none() || extra["pkg_homepage"].is_null());
    assert!(extra.get("pkg_repo_url").is_none() || extra["pkg_repo_url"].is_null());
}
```

Add to `npm.rs`:

```rust
#[cfg(test)]
#[path = "npm_tests.rs"]
mod tests;
```

- [ ] **Step 3: Add `build_extra` to `pypi.rs`**

In `src/extract/verticals/pypi.rs`, add before `extract()`:

```rust
fn build_extra(
    name: &str,
    version: &str,
    license: &str,
    author: &str,
    keywords: &[&str],
    home_page: &str,
    requires_python: &str,
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "pkg_registry": "pypi",
        "pkg_name": name,
        "pkg_version": version,
        "pkg_language": "python",
    });
    if !license.is_empty() {
        obj["pkg_license"] = serde_json::Value::String(license.to_string());
    }
    if !author.is_empty() {
        obj["pkg_author"] = serde_json::Value::String(author.to_string());
    }
    if !keywords.is_empty() {
        obj["pkg_keywords"] = serde_json::json!(keywords);
    }
    if !home_page.is_empty() {
        obj["pkg_homepage"] = serde_json::Value::String(home_page.to_string());
    }
    if !requires_python.is_empty() {
        obj["pypi_requires_python"] = serde_json::Value::String(requires_python.to_string());
    }
    obj
}
```

In `extract()`, compute extra and add to `ScrapedDoc`:

```rust
let extra = build_extra(pkg_name, version, license, author, &keywords, home_page, requires_python);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title,
    extractor_name: INFO.name,
    extractor_version: 2,
    structured: Some(data),
    follow_crawl_urls,
    extra: Some(extra),
})
```

- [ ] **Step 4: Create `pypi_tests.rs`**

Create `src/extract/verticals/pypi_tests.rs`:

```rust
use super::build_extra;

#[test]
fn build_extra_pypi() {
    let extra = build_extra("requests", "2.32.0", "Apache-2.0", "Kenneth Reitz",
        &["http", "client"], "https://requests.readthedocs.io", ">=3.8");
    assert_eq!(extra["pkg_registry"], "pypi");
    assert_eq!(extra["pkg_name"], "requests");
    assert_eq!(extra["pkg_language"], "python");
    assert_eq!(extra["pypi_requires_python"], ">=3.8");
}

#[test]
fn build_extra_pypi_no_requires_python() {
    let extra = build_extra("simple", "1.0.0", "MIT", "dev", &[], "", "");
    assert!(extra.get("pypi_requires_python").is_none() || extra["pypi_requires_python"].is_null());
}
```

Add to `pypi.rs`:

```rust
#[cfg(test)]
#[path = "pypi_tests.rs"]
mod tests;
```

- [ ] **Step 5: Add `build_extra` to `crates_io.rs`**

In `src/extract/verticals/crates_io.rs`, add before the `extract()` function. The relevant data comes from the `data` JSON value — look at how `build_markdown` uses it. The fields are at `data["crate"]`, `data["versions"][0]`, and `data["keywords"]`.

Add this function:

```rust
fn build_extra(data: &serde_json::Value) -> serde_json::Value {
    let krate = &data["crate"];
    let ver = &data["versions"][0];
    let name = krate["name"].as_str().unwrap_or("");
    let max_version = krate["max_stable_version"]
        .as_str()
        .or_else(|| krate["newest_version"].as_str())
        .unwrap_or("");
    let license = ver["license"].as_str().unwrap_or("");
    let downloads = krate["downloads"].as_u64().unwrap_or(0);
    let homepage = krate["homepage"].as_str().unwrap_or("");
    let repository = krate["repository"].as_str().unwrap_or("");
    let msrv = ver["rust_version"].as_str().unwrap_or("");
    let edition = ver["edition"].as_str().unwrap_or("");
    let keywords: Vec<&str> = data["keywords"]
        .as_array()
        .map(|a| a.iter().filter_map(|k| k["keyword"].as_str()).collect())
        .unwrap_or_default();

    let mut obj = serde_json::json!({
        "pkg_registry": "crates_io",
        "pkg_name": name,
        "pkg_version": max_version,
        "pkg_language": "rust",
    });
    if !license.is_empty() {
        obj["pkg_license"] = serde_json::Value::String(license.to_string());
    }
    if !keywords.is_empty() {
        obj["pkg_keywords"] = serde_json::json!(keywords);
    }
    if downloads > 0 {
        obj["pkg_downloads"] = serde_json::json!(downloads);
    }
    if !homepage.is_empty() {
        obj["pkg_homepage"] = serde_json::Value::String(homepage.to_string());
    }
    if !repository.is_empty() {
        obj["pkg_repo_url"] = serde_json::Value::String(repository.to_string());
    }
    if !msrv.is_empty() {
        obj["crate_msrv"] = serde_json::Value::String(msrv.to_string());
    }
    if !edition.is_empty() {
        obj["crate_edition"] = serde_json::Value::String(edition.to_string());
    }
    obj
}
```

In `extract()`, compute `extra` before building `ScrapedDoc`. `data` is in scope at the `Ok(ScrapedDoc { ... })` at line 85:

```rust
let extra = build_extra(&data);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown,
    title,
    extractor_name: INFO.name,
    extractor_version: 3,
    structured: Some(data),
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

Add a test to `crates_io_tests.rs`:

```rust
#[test]
fn build_extra_crates_io() {
    let data = serde_json::json!({
        "crate": {
            "name": "serde",
            "max_stable_version": "1.0.200",
            "downloads": 5000000u64,
            "homepage": "",
            "repository": "https://github.com/serde-rs/serde",
        },
        "versions": [{"license": "MIT OR Apache-2.0", "rust_version": "1.65", "edition": "2021", "features": {}}],
        "keywords": [{"keyword": "serialization"}, {"keyword": "deserialization"}],
        "categories": [],
    });
    let extra = super::build_extra(&data);
    assert_eq!(extra["pkg_registry"], "crates_io");
    assert_eq!(extra["pkg_name"], "serde");
    assert_eq!(extra["pkg_language"], "rust");
    assert_eq!(extra["crate_msrv"], "1.65");
    assert_eq!(extra["crate_edition"], "2021");
    assert_eq!(extra["pkg_repo_url"], "https://github.com/serde-rs/serde");
}
```

- [ ] **Step 6: Run package registry tests**

```bash
cargo test npm_tests pypi_tests crates_io_tests 2>&1 | tail -20
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add src/extract/verticals/npm.rs \
        src/extract/verticals/npm_tests.rs \
        src/extract/verticals/pypi.rs \
        src/extract/verticals/pypi_tests.rs \
        src/extract/verticals/crates_io.rs \
        src/extract/verticals/crates_io_tests.rs
git commit -m "feat(verticals): add pkg_* extra payload to npm, pypi, crates_io"
```

---

## Task 6: `docs_rs` — add `build_extra` with item count

**Files:**
- Modify: `src/extract/verticals/docs_rs.rs`
- Modify: `src/extract/verticals/docs_rs_tests.rs`

`docs_rs.rs` is special: the `item_count` lives inside `rustdoc_to_markdown()` as a local `count` variable, not in `extract()`. The fix is to refactor `rustdoc_to_markdown` to also return the count, or to compute it separately in `extract()` by inspecting `data["index"]`.

The cleaner approach: compute `item_count` in `extract()` by counting public index entries directly from the JSON, before calling `rustdoc_to_markdown`. Then `build_extra` receives the count as a parameter.

- [ ] **Step 1: Add `build_extra` and compute item count in `docs_rs.rs`**

Add this function before `extract()`:

```rust
fn build_extra(name: &str, version: &str, item_count: usize) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "pkg_registry": "docs_rs",
        "pkg_name": name,
        "pkg_version": version,
        "pkg_language": "rust",
    });
    if item_count > 0 {
        obj["docrs_item_count"] = serde_json::json!(item_count);
    }
    obj
}

/// Count public items with documentation in a rustdoc JSON index.
/// This mirrors the filtering logic in `rustdoc_to_markdown`.
fn count_doc_items(data: &serde_json::Value) -> usize {
    let Some(index) = data["index"].as_object() else {
        return 0;
    };
    index
        .values()
        .filter(|item| {
            item["visibility"].as_str() == Some("public")
                && item["docs"].as_str().filter(|d| !d.is_empty()).is_some()
                && !item["inner"]
                    .as_object()
                    .and_then(|o| o.keys().next())
                    .map(|k| should_skip_kind(k))
                    .unwrap_or(true)
        })
        .count()
}
```

In `extract()`, after `data` is available, compute item_count and extra, then add `extra` to `ScrapedDoc`. Find the function signature and locate where `data` is produced. In `docs_rs.rs`, `extract()` fetches both crate metadata and rustdoc JSON. Look for the `Ok(ScrapedDoc { ... })` return and update:

```rust
let item_count = count_doc_items(&rustdoc_data);  // wherever rustdoc_data is in scope
let version = rustdoc_data["crate_version"].as_str().unwrap_or("?");
let extra = build_extra(name, version, item_count);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown,
    title,
    extractor_name: INFO.name,
    extractor_version: 2,
    structured: Some(rustdoc_data),
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

(Adjust variable names to match the actual variable names in `docs_rs.rs`. Read the file to confirm the variable holding the rustdoc JSON response.)

- [ ] **Step 2: Add test to `docs_rs_tests.rs`**

In `src/extract/verticals/docs_rs_tests.rs`, add:

```rust
#[test]
fn build_extra_docs_rs() {
    let extra = super::build_extra("serde", "1.0.200", 142);
    assert_eq!(extra["pkg_registry"], "docs_rs");
    assert_eq!(extra["pkg_name"], "serde");
    assert_eq!(extra["pkg_language"], "rust");
    assert_eq!(extra["docrs_item_count"], 142u64);
}

#[test]
fn count_doc_items_empty() {
    let data = serde_json::json!({ "index": {} });
    assert_eq!(super::count_doc_items(&data), 0);
}

#[test]
fn count_doc_items_skips_non_public() {
    let data = serde_json::json!({
        "index": {
            "1": {
                "visibility": "public",
                "docs": "A documented item",
                "inner": { "function": {} }
            },
            "2": {
                "visibility": "restricted",
                "docs": "Not shown",
                "inner": { "function": {} }
            }
        }
    });
    assert_eq!(super::count_doc_items(&data), 1);
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test docs_rs_tests 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add src/extract/verticals/docs_rs.rs src/extract/verticals/docs_rs_tests.rs
git commit -m "feat(verticals): add pkg_*/docrs_* extra payload to docs_rs"
```

---

## Task 7: Docker Hub and HuggingFace Model extractors

**Files:**
- Modify: `src/extract/verticals/docker_hub.rs`
- Modify: `src/extract/verticals/huggingface_model.rs`
- Create: `src/extract/verticals/docker_hub_tests.rs`
- Create: `src/extract/verticals/huggingface_model_tests.rs`

- [ ] **Step 1: Add `build_extra` to `docker_hub.rs`**

In `docker_hub.rs`, add before `extract()`. The `namespace` and `repo` variables are parsed from the URL path at lines 47–56; `data` holds the API response. All needed variables are in scope when `ScrapedDoc` is built:

```rust
fn build_extra(
    namespace: &str,
    img_name: &str,
    full_name: &str,
    pull_count: u64,
    star_count: u64,
    is_official: bool,
    last_updated: &str,
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "docker_namespace": namespace,
        "docker_image": img_name,
        "docker_full_name": full_name,
        "docker_pulls": pull_count,
        "docker_stars": star_count,
        "docker_is_official": is_official,
    });
    if !last_updated.is_empty() {
        obj["docker_last_updated"] = serde_json::Value::String(last_updated.to_string());
    }
    obj
}
```

Add before `Ok(ScrapedDoc { ... })` in `extract()`:

```rust
let extra = build_extra(namespace, img_name, full_name, pull_count, star_count, is_official, last_updated);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title,
    extractor_name: INFO.name,
    extractor_version: 2,
    structured: Some(data),
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

Add to `docker_hub.rs`:

```rust
#[cfg(test)]
#[path = "docker_hub_tests.rs"]
mod tests;
```

Create `src/extract/verticals/docker_hub_tests.rs`:

```rust
use super::build_extra;

#[test]
fn build_extra_official_image() {
    let extra = build_extra("library", "nginx", "library/nginx", 1_000_000_000u64, 12345u64, true, "2024-01-01T00:00:00Z");
    assert_eq!(extra["docker_namespace"], "library");
    assert_eq!(extra["docker_image"], "nginx");
    assert_eq!(extra["docker_full_name"], "library/nginx");
    assert_eq!(extra["docker_is_official"], true);
    assert_eq!(extra["docker_pulls"], 1_000_000_000u64);
}

#[test]
fn build_extra_community_image() {
    let extra = build_extra("myorg", "myapp", "myorg/myapp", 500u64, 10u64, false, "");
    assert_eq!(extra["docker_is_official"], false);
    assert!(extra.get("docker_last_updated").is_none() || extra["docker_last_updated"].is_null());
}
```

- [ ] **Step 2: Add `build_extra` to `huggingface_model.rs`**

In `huggingface_model.rs`, all needed data is in scope at the `Ok(ScrapedDoc { ... })` — `id`, `pipeline_tag`, `library_name`, `downloads`, `likes`, `tags`, `org`, `model` are available.

Add before `extract()`:

```rust
fn build_extra(
    model_id: &str,
    org: &str,
    pipeline_tag: &str,
    library_name: &str,
    downloads: u64,
    likes: u64,
    tags: &[&str],
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "hf_model_id": model_id,
        "hf_org": org,
        "hf_downloads": downloads,
        "hf_likes": likes,
    });
    if !pipeline_tag.is_empty() {
        obj["hf_task"] = serde_json::Value::String(pipeline_tag.to_string());
    }
    if !library_name.is_empty() {
        obj["hf_library"] = serde_json::Value::String(library_name.to_string());
    }
    if !tags.is_empty() {
        obj["hf_tags"] = serde_json::json!(tags);
    }
    obj
}
```

In `extract()`, add before `Ok(ScrapedDoc { ... })`:

```rust
let extra = build_extra(id, org, pipeline_tag, library_name, downloads, likes, &tags);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title,
    extractor_name: INFO.name,
    extractor_version: 2,
    structured: Some(data),
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

Add to `huggingface_model.rs`:

```rust
#[cfg(test)]
#[path = "huggingface_model_tests.rs"]
mod tests;
```

Create `src/extract/verticals/huggingface_model_tests.rs`:

```rust
use super::build_extra;

#[test]
fn build_extra_hf_model() {
    let extra = build_extra(
        "mistralai/Mistral-7B-v0.1",
        "mistralai",
        "text-generation",
        "transformers",
        5_000_000u64,
        8000u64,
        &["pytorch", "safetensors"],
    );
    assert_eq!(extra["hf_model_id"], "mistralai/Mistral-7B-v0.1");
    assert_eq!(extra["hf_org"], "mistralai");
    assert_eq!(extra["hf_task"], "text-generation");
    assert_eq!(extra["hf_library"], "transformers");
    assert_eq!(extra["hf_downloads"], 5_000_000u64);
}

#[test]
fn build_extra_hf_no_task() {
    let extra = build_extra("org/model", "org", "", "", 0, 0, &[]);
    assert!(extra.get("hf_task").is_none() || extra["hf_task"].is_null());
    assert!(extra.get("hf_library").is_none() || extra["hf_library"].is_null());
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test docker_hub_tests huggingface_model_tests 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add src/extract/verticals/docker_hub.rs \
        src/extract/verticals/docker_hub_tests.rs \
        src/extract/verticals/huggingface_model.rs \
        src/extract/verticals/huggingface_model_tests.rs
git commit -m "feat(verticals): add docker_* and hf_* extra payload to docker_hub and huggingface_model"
```

---

## Task 8: DEV Community and Shopify extractors

**Files:**
- Modify: `src/extract/verticals/dev_to.rs`
- Modify: `src/extract/verticals/shopify.rs`
- Create: `src/extract/verticals/dev_to_tests.rs`
- Create: `src/extract/verticals/shopify_tests.rs`

- [ ] **Step 1: Add `build_extra` to `dev_to.rs`**

In `dev_to.rs`, the variables `username`, `tags`, `reactions`, `reading_time`, and `data["published_at"]` are available when `ScrapedDoc` is built.

Add before `extract()`:

```rust
fn build_extra(
    username: &str,
    tags: &[&str],
    reactions: u64,
    reading_time_mins: u64,
    published_at: &str,
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "devto_author": username,
        "devto_reactions": reactions,
        "devto_reading_time_mins": reading_time_mins,
    });
    if !tags.is_empty() {
        obj["devto_tags"] = serde_json::json!(tags);
    }
    if !published_at.is_empty() {
        obj["devto_published_at"] = serde_json::Value::String(published_at.to_string());
    }
    obj
}
```

In `extract()`, get `published_at` from `data` and add extra:

```rust
let published_at = data["published_at"].as_str().unwrap_or("");
let extra = build_extra(username, &tags, reactions, reading_time, published_at);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title,
    extractor_name: INFO.name,
    extractor_version: 2,
    structured: Some(data),
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

Add to `dev_to.rs`:

```rust
#[cfg(test)]
#[path = "dev_to_tests.rs"]
mod tests;
```

Create `src/extract/verticals/dev_to_tests.rs`:

```rust
use super::build_extra;

#[test]
fn build_extra_dev_to() {
    let extra = build_extra("dhravya", &["rust", "webdev"], 420u64, 8u64, "2024-03-01T10:00:00Z");
    assert_eq!(extra["devto_author"], "dhravya");
    assert_eq!(extra["devto_reactions"], 420u64);
    assert_eq!(extra["devto_reading_time_mins"], 8u64);
    assert_eq!(extra["devto_published_at"], "2024-03-01T10:00:00Z");
}

#[test]
fn build_extra_dev_to_no_tags() {
    let extra = build_extra("user", &[], 0, 0, "");
    assert!(extra.get("devto_tags").is_none() || extra["devto_tags"].is_null());
    assert!(extra.get("devto_published_at").is_none() || extra["devto_published_at"].is_null());
}
```

- [ ] **Step 2: Add `build_extra` to `shopify.rs`**

In `shopify.rs`, `host` and `handle` are parsed in `extract()`, and `vendor`/`product_type` come from `product["vendor"]`/`product["product_type"]`.

Add before `extract()`:

```rust
fn build_extra(host: &str, vendor: &str, product_type: &str, handle: &str) -> serde_json::Value {
    let mut obj = serde_json::json!({ "shop_host": host });
    if !vendor.is_empty() {
        obj["shop_vendor"] = serde_json::Value::String(vendor.to_string());
    }
    if !product_type.is_empty() {
        obj["shop_product_type"] = serde_json::Value::String(product_type.to_string());
    }
    if !handle.is_empty() {
        obj["shop_handle"] = serde_json::Value::String(handle.to_string());
    }
    obj
}
```

In `extract()`, add before `Ok(ScrapedDoc { ... })`:

```rust
let extra = build_extra(&host, vendor, product_type, &handle);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title,
    extractor_name: INFO.name,
    extractor_version: 2,
    structured: Some(data),
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

Add to `shopify.rs`:

```rust
#[cfg(test)]
#[path = "shopify_tests.rs"]
mod tests;
```

Create `src/extract/verticals/shopify_tests.rs`:

```rust
use super::build_extra;

#[test]
fn build_extra_shopify() {
    let extra = build_extra("shop.example.com", "ACME Corp", "Widgets", "blue-widget");
    assert_eq!(extra["shop_host"], "shop.example.com");
    assert_eq!(extra["shop_vendor"], "ACME Corp");
    assert_eq!(extra["shop_product_type"], "Widgets");
    assert_eq!(extra["shop_handle"], "blue-widget");
}

#[test]
fn build_extra_shopify_empty_vendor() {
    let extra = build_extra("shop.example.com", "", "", "handle");
    assert!(extra.get("shop_vendor").is_none() || extra["shop_vendor"].is_null());
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test dev_to_tests shopify_tests 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add src/extract/verticals/dev_to.rs \
        src/extract/verticals/dev_to_tests.rs \
        src/extract/verticals/shopify.rs \
        src/extract/verticals/shopify_tests.rs
git commit -m "feat(verticals): add devto_* and shop_* extra payload to dev_to and shopify"
```

---

## Task 9: Hacker News, Stack Overflow, and arXiv extractors

**Files:**
- Modify: `src/extract/verticals/hackernews.rs`
- Modify: `src/extract/verticals/stackoverflow.rs`
- Modify: `src/extract/verticals/arxiv.rs`
- Modify: `src/extract/verticals/hackernews_tests.rs`
- Modify: `src/extract/verticals/stackoverflow_tests.rs`
- Modify: `src/extract/verticals/arxiv_tests.rs`

- [ ] **Step 1: Add `build_extra` to `hackernews.rs`**

HN type inference: `item_type == "job"` → `"job"`, title starts with `"Ask HN:"` → `"ask_hn"`, title starts with `"Show HN:"` → `"show_hn"`, else → `"story"`.

Add before `build_scraped_doc()`:

```rust
fn infer_hn_type(item_type: Option<&str>, title: &str) -> &'static str {
    if item_type == Some("job") {
        return "job";
    }
    if title.starts_with("Ask HN:") {
        return "ask_hn";
    }
    if title.starts_with("Show HN:") {
        return "show_hn";
    }
    "story"
}

fn build_extra(
    item_id: u64,
    hn_type: &str,
    author: &str,
    points: u64,
    comment_count: u64,
    created_at: &str,
    external_url: Option<&str>,
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "hn_id": item_id,
        "hn_type": hn_type,
        "hn_author": author,
        "hn_points": points,
        "hn_comment_count": comment_count,
    });
    if !created_at.is_empty() {
        obj["hn_created_at"] = serde_json::Value::String(created_at.to_string());
    }
    if let Some(u) = external_url {
        obj["hn_external_url"] = serde_json::Value::String(u.to_string());
    }
    obj
}
```

In `build_scraped_doc()`, add extra calculation and field:

```rust
let hn_type = infer_hn_type(item.item_type.as_deref(), &title);
let extra = build_extra(
    item_id,
    hn_type,
    author,
    points,
    comment_count,
    created_at,
    item.url.as_deref(),
);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title: Some(title),
    extractor_name: INFO.name,
    extractor_version: 2,  // bump from 1
    structured: Some(structured),
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

Add tests to `hackernews_tests.rs`:

```rust
#[test]
fn infer_hn_type_ask() {
    assert_eq!(super::infer_hn_type(None, "Ask HN: How do I learn Rust?"), "ask_hn");
}

#[test]
fn infer_hn_type_show() {
    assert_eq!(super::infer_hn_type(None, "Show HN: My new project"), "show_hn");
}

#[test]
fn infer_hn_type_job() {
    assert_eq!(super::infer_hn_type(Some("job"), "Senior Eng at BigCo"), "job");
}

#[test]
fn infer_hn_type_story() {
    assert_eq!(super::infer_hn_type(Some("story"), "Rust is fast"), "story");
}

#[test]
fn build_extra_hackernews() {
    let extra = super::build_extra(42000001u64, "story", "pg", 500u64, 123u64,
        "2024-01-01T00:00:00.000Z", Some("https://example.com"));
    assert_eq!(extra["hn_id"], 42000001u64);
    assert_eq!(extra["hn_type"], "story");
    assert_eq!(extra["hn_author"], "pg");
    assert_eq!(extra["hn_external_url"], "https://example.com");
}
```

- [ ] **Step 2: Add `build_extra` to `stackoverflow.rs`**

`so_is_answered` is stored as a string `"true"` or `"false"` for keyword-index compatibility.

Add before `build_scraped_doc()`:

```rust
fn build_extra(
    question_id: u64,
    tags: &[&str],
    score: i64,
    view_count: u64,
    is_answered: bool,
    author: &str,
    answer_count: u64,
    created_at: &str,
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "so_question_id": question_id,
        "so_score": score,
        "so_view_count": view_count,
        // Stored as string for keyword index compatibility
        "so_is_answered": if is_answered { "true" } else { "false" },
        "so_answer_count": answer_count,
    });
    if !tags.is_empty() {
        obj["so_tags"] = serde_json::json!(tags);
    }
    if !author.is_empty() {
        obj["so_author"] = serde_json::Value::String(author.to_string());
    }
    if !created_at.is_empty() {
        obj["so_created_at"] = serde_json::Value::String(created_at.to_string());
    }
    obj
}
```

In `build_scraped_doc()`, compute extra and add to `ScrapedDoc`:

```rust
let extra = build_extra(
    question["question_id"].as_u64().unwrap_or(0),
    &tags,
    score,
    view_count,
    is_answered,
    q_author,
    answer_count,
    &date_str,
);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title: Some(title),
    extractor_name: INFO.name,
    extractor_version: 2,  // bump from 1
    structured: Some(structured),
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

Add tests to `stackoverflow_tests.rs`:

```rust
#[test]
fn build_extra_so() {
    let extra = super::build_extra(12345678u64, &["rust", "lifetime"], 142i64, 5000u64, true, "shepmaster", 3u64, "2024-01-15");
    assert_eq!(extra["so_question_id"], 12345678u64);
    assert_eq!(extra["so_is_answered"], "true");
    assert_eq!(extra["so_author"], "shepmaster");
    assert_eq!(extra["so_score"], 142i64);
}

#[test]
fn build_extra_so_unanswered() {
    let extra = super::build_extra(99u64, &[], 0i64, 10u64, false, "", 0u64, "");
    assert_eq!(extra["so_is_answered"], "false");
    assert!(extra.get("so_author").is_none() || extra["so_author"].is_null());
}
```

- [ ] **Step 3: Add `build_extra` to `arxiv.rs`**

In `build_scraped_doc()`, all needed data is already available: `arxiv_id`, `authors`, `categories`, `published`, `pdf_url`.

Add before `build_scraped_doc()`:

```rust
fn build_extra(
    arxiv_id: &str,
    authors: &[String],
    categories: &[String],
    published: &str,
    pdf_url: &str,
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "arxiv_id": arxiv_id,
    });
    if !authors.is_empty() {
        obj["arxiv_authors"] = serde_json::json!(authors);
    }
    if !categories.is_empty() {
        obj["arxiv_categories"] = serde_json::json!(categories);
    }
    if !published.is_empty() {
        obj["arxiv_published"] = serde_json::Value::String(published.to_string());
    }
    if !pdf_url.is_empty() {
        obj["arxiv_pdf_url"] = serde_json::Value::String(pdf_url.to_string());
    }
    obj
}
```

In `build_scraped_doc()`, add before `Ok(ScrapedDoc { ... })`:

```rust
let extra = build_extra(arxiv_id, &authors, &categories, &published, &pdf_url);
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title: Some(title),
    extractor_name: INFO.name,
    extractor_version: 2,  // bump from 1
    structured: Some(structured),
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

Add tests to `arxiv_tests.rs`:

```rust
#[test]
fn build_extra_arxiv() {
    let extra = super::build_extra(
        "2312.00752",
        &["LeCun Y".to_string(), "Bengio Y".to_string()],
        &["cs.LG".to_string(), "stat.ML".to_string()],
        "2023-12-01T00:00:00Z",
        "https://arxiv.org/pdf/2312.00752",
    );
    assert_eq!(extra["arxiv_id"], "2312.00752");
    assert!(extra["arxiv_authors"].is_array());
    assert!(extra["arxiv_categories"].is_array());
}

#[test]
fn build_extra_arxiv_no_optional_fields() {
    let extra = super::build_extra("1234.56789", &[], &[], "", "");
    assert_eq!(extra["arxiv_id"], "1234.56789");
    assert!(extra.get("arxiv_authors").is_none() || extra["arxiv_authors"].is_null());
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test hackernews_tests stackoverflow_tests arxiv_tests 2>&1 | tail -20
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/extract/verticals/hackernews.rs \
        src/extract/verticals/hackernews_tests.rs \
        src/extract/verticals/stackoverflow.rs \
        src/extract/verticals/stackoverflow_tests.rs \
        src/extract/verticals/arxiv.rs \
        src/extract/verticals/arxiv_tests.rs
git commit -m "feat(verticals): add hn_*/so_*/arxiv_* extra payload to hackernews, stackoverflow, arxiv"
```

---

## Task 10: Amazon and eBay extractors

**Files:**
- Modify: `src/extract/verticals/amazon.rs`
- Modify: `src/extract/verticals/ebay.rs`
- Create: `src/extract/verticals/amazon_tests.rs`
- Create: `src/extract/verticals/ebay_tests.rs`

Both extractors parse JSON-LD from HTML (not a clean API response). The `build_scraped_doc` function in each receives `jsonld: Option<serde_json::Value>` and `asin/item_id: Option<String>`. Data comes from `jsonld["offers"][...]`, `jsonld["brand"]["name"]`, etc.

- [ ] **Step 1: Add `build_extra` to `amazon.rs`**

In `amazon.rs`, add before `build_scraped_doc()`:

```rust
fn build_extra(jsonld: Option<&serde_json::Value>, asin: Option<&str>) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(asin_val) = asin {
        obj.insert("amz_asin".to_string(), serde_json::Value::String(asin_val.to_string()));
    }
    if let Some(j) = jsonld {
        if let Some(brand) = j["brand"]["name"].as_str() {
            obj.insert("amz_brand".to_string(), serde_json::Value::String(brand.to_string()));
        }
        let price = j["offers"]["price"].as_str();
        let currency = j["offers"]["priceCurrency"].as_str();
        if let Some(p) = price {
            obj.insert("amz_price".to_string(), serde_json::Value::String(p.to_string()));
        }
        if let Some(c) = currency {
            obj.insert("amz_currency".to_string(), serde_json::Value::String(c.to_string()));
        }
        if let Some(avail) = j["offers"]["availability"].as_str() {
            let short = avail.split('/').next_back().unwrap_or(avail);
            obj.insert("amz_availability".to_string(), serde_json::Value::String(short.to_string()));
        }
        if let Some(r) = j["aggregateRating"]["ratingValue"].as_f64() {
            obj.insert("amz_rating".to_string(), serde_json::json!(r));
        }
        if let Some(rc) = j["aggregateRating"]["reviewCount"].as_u64() {
            obj.insert("amz_review_count".to_string(), serde_json::json!(rc));
        }
    }
    serde_json::Value::Object(obj)
}
```

In `build_scraped_doc()`, compute and add extra:

```rust
let extra = build_extra(jsonld.as_ref(), asin.as_deref());
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title,
    extractor_name: INFO.name,
    extractor_version: 2,
    structured: jsonld,
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

Add to `amazon.rs`:

```rust
#[cfg(test)]
#[path = "amazon_tests.rs"]
mod tests;
```

Create `src/extract/verticals/amazon_tests.rs`:

```rust
use super::build_extra;

#[test]
fn build_extra_amazon_with_jsonld() {
    let jsonld = serde_json::json!({
        "brand": { "name": "ACME" },
        "offers": {
            "price": "29.99",
            "priceCurrency": "USD",
            "availability": "https://schema.org/InStock",
        },
        "aggregateRating": {
            "ratingValue": 4.5,
            "reviewCount": 1234u64,
        }
    });
    let extra = build_extra(Some(&jsonld), Some("B0ABCDEFGH"));
    assert_eq!(extra["amz_asin"], "B0ABCDEFGH");
    assert_eq!(extra["amz_brand"], "ACME");
    assert_eq!(extra["amz_price"], "29.99");
    assert_eq!(extra["amz_currency"], "USD");
    assert_eq!(extra["amz_availability"], "InStock");
    assert_eq!(extra["amz_review_count"], 1234u64);
}

#[test]
fn build_extra_amazon_minimal() {
    let extra = build_extra(None, Some("B0000000000"));
    assert_eq!(extra["amz_asin"], "B0000000000");
    assert!(extra.get("amz_brand").is_none() || extra["amz_brand"].is_null());
}
```

- [ ] **Step 2: Add `build_extra` to `ebay.rs`**

In `ebay.rs`, same pattern — `jsonld` and `item_id` are parameters to `build_scraped_doc()`.

Add before `build_scraped_doc()`:

```rust
fn build_extra(jsonld: Option<&serde_json::Value>, item_id: Option<&str>) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(id) = item_id {
        obj.insert("ebay_item_id".to_string(), serde_json::Value::String(id.to_string()));
    }
    if let Some(j) = jsonld {
        if let Some(brand) = j["brand"]["name"].as_str() {
            obj.insert("ebay_brand".to_string(), serde_json::Value::String(brand.to_string()));
        }
        let price = j["offers"]["price"].as_str();
        if let Some(p) = price {
            obj.insert("ebay_price".to_string(), serde_json::Value::String(p.to_string()));
        }
        if let Some(cond) = j["offers"]["itemCondition"].as_str() {
            let short = cond.split('/').next_back().unwrap_or(cond);
            let clean = short.trim_end_matches("Condition");
            obj.insert("ebay_condition".to_string(), serde_json::Value::String(clean.to_string()));
        }
        if let Some(avail) = j["offers"]["availability"].as_str() {
            let short = avail.split('/').next_back().unwrap_or(avail);
            obj.insert("ebay_availability".to_string(), serde_json::Value::String(short.to_string()));
        }
        if let Some(r) = j["aggregateRating"]["ratingValue"].as_f64() {
            obj.insert("ebay_rating".to_string(), serde_json::json!(r));
        }
        if let Some(rc) = j["aggregateRating"]["reviewCount"].as_u64() {
            obj.insert("ebay_review_count".to_string(), serde_json::json!(rc));
        }
    }
    serde_json::Value::Object(obj)
}
```

In `build_scraped_doc()`:

```rust
let extra = build_extra(jsonld.as_ref(), item_id.as_deref());
Ok(ScrapedDoc {
    url: url.to_string(),
    markdown: md,
    title,
    extractor_name: INFO.name,
    extractor_version: 2,
    structured: jsonld,
    follow_crawl_urls: vec![],
    extra: Some(extra),
})
```

Add to `ebay.rs`:

```rust
#[cfg(test)]
#[path = "ebay_tests.rs"]
mod tests;
```

Create `src/extract/verticals/ebay_tests.rs`:

```rust
use super::build_extra;

#[test]
fn build_extra_ebay_condition_cleaned() {
    let jsonld = serde_json::json!({
        "offers": {
            "itemCondition": "https://schema.org/NewCondition",
            "availability": "https://schema.org/InStock",
        }
    });
    let extra = build_extra(Some(&jsonld), Some("123456789012"));
    assert_eq!(extra["ebay_item_id"], "123456789012");
    assert_eq!(extra["ebay_condition"], "New");
    assert_eq!(extra["ebay_availability"], "InStock");
}

#[test]
fn build_extra_ebay_no_jsonld() {
    let extra = build_extra(None, Some("987654321098"));
    assert_eq!(extra["ebay_item_id"], "987654321098");
    assert!(extra.get("ebay_brand").is_none() || extra["ebay_brand"].is_null());
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test amazon_tests ebay_tests 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add src/extract/verticals/amazon.rs \
        src/extract/verticals/amazon_tests.rs \
        src/extract/verticals/ebay.rs \
        src/extract/verticals/ebay_tests.rs
git commit -m "feat(verticals): add amz_*/ebay_* extra payload to amazon and ebay"
```

---

## Task 11: Add vertical payload indexes to Qdrant

**Files:**
- Modify: `src/vector/ops/tei/qdrant_store/payload_indexes.rs`

This adds the vertical-specific keyword and integer indexes from the spec. The `ensure_payload_indexes` function is already structured for easy extension.

- [ ] **Step 1: Add vertical keyword fields**

In `payload_indexes.rs`, extend the `keyword_fields` array:

```rust
let keyword_fields = [
    // ... existing fields ...
    "url",
    "domain",
    "source_type",
    "gh_file_language",
    "chunking_method",
    "extractor_name",
    "provider",
    "git_host",
    "git_owner",
    "git_repo",
    "git_content_kind",
    "git_state",
    "git_author",
    "git_file_language",
    // Vertical extractor fields (spec: docs/specs/vertical-extractor-metadata.md)
    // Package registry (npm, pypi, crates_io, docs_rs)
    "pkg_registry",
    "pkg_name",
    "pkg_language",
    "pkg_license",
    "pkg_author",
    // HuggingFace model
    "hf_task",
    "hf_library",
    // Stack Overflow
    "so_is_answered",
    // Hacker News
    "hn_type",
    "hn_author",
    // arXiv
    "arxiv_id",
    // DEV Community
    "devto_author",
];
```

- [ ] **Step 2: Add vertical integer indexes**

After the `schema_version_url` future block and before the `datetime_url` block, add integer indexes for `so_question_id`:

```rust
let so_question_url = index_url.clone();
futures.push(Box::pin(async move {
    client
        .put(&so_question_url)
        .json(&serde_json::json!({
            "field_name": "so_question_id",
            "field_schema": "integer"
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}));
```

Also update the `Vec::with_capacity` call to account for the added futures:

```rust
// Change from:
let mut futures: Vec<IndexFut<'_>> = Vec::with_capacity(keyword_fields.len() + 3);
// To:
let mut futures: Vec<IndexFut<'_>> = Vec::with_capacity(keyword_fields.len() + 4);
```

- [ ] **Step 3: Verify compile**

```bash
cargo check 2>&1 | grep "error\[" | head -10
```

Expected: 0 errors.

- [ ] **Step 4: Commit**

```bash
git add src/vector/ops/tei/qdrant_store/payload_indexes.rs
git commit -m "feat(indexes): add vertical extractor keyword and integer payload indexes to Qdrant"
```

---

## Task 12: Full verification pass and spec update

**Files:**
- Verify: all modified files compile and tests pass
- Modify: `docs/specs/vertical-extractor-metadata.md` — update Implementation Status table

- [ ] **Step 1: Run the full test suite**

```bash
cargo test 2>&1 | tail -40
```

Expected: all tests pass, 0 failures.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy 2>&1 | grep "^error" | head -20
```

Expected: 0 errors. Fix any warnings that are in files you modified.

- [ ] **Step 3: Check monolith line counts**

```bash
wc -l src/extract/verticals/*.rs | sort -rn | head -20
```

Verify no file exceeds 500 lines. If any does, split the `build_extra` function into a sidecar file (e.g., `crates_io_payload.rs` declared with `mod crates_io_payload;` inside `crates_io.rs`).

- [ ] **Step 4: Update the Implementation Status table in the spec**

In `docs/specs/vertical-extractor-metadata.md`, update the Implementation Status table from:

```markdown
| extra field on ScrapedDoc          | pending |
| extra on ScrapeResult              | pending |
| Scrape CLI PreparedDoc path        | pending |
| GitHub verticals (git_*)           | pending |
| npm                                | pending |
| pypi                               | pending |
| crates_io                          | pending |
| docs_rs                            | pending |
| docker_hub                         | pending |
| huggingface_model                  | pending |
| dev_to                             | pending |
| shopify                            | pending |
| hackernews                         | pending |
| stackoverflow                      | pending |
| arxiv                              | pending |
| amazon                             | pending |
| ebay                               | pending |
| Payload indexes                    | pending |
```

To:

```markdown
| extra field on ScrapedDoc          | done |
| extra on ScrapeResult              | done |
| Scrape CLI PreparedDoc path        | done |
| GitHub verticals (git_*)           | done |
| npm                                | done |
| pypi                               | done |
| crates_io                          | done |
| docs_rs                            | done |
| docker_hub                         | done |
| huggingface_model                  | done |
| dev_to                             | done |
| shopify                            | done |
| hackernews                         | done |
| stackoverflow                      | done |
| arxiv                              | done |
| amazon                             | done |
| ebay                               | done |
| Payload indexes                    | done |
```

- [ ] **Step 5: Final commit**

```bash
git add docs/specs/vertical-extractor-metadata.md
git commit -m "docs: mark all vertical extractor metadata items as done in spec"
```

---

## Self-Review Checklist

**Spec coverage:**
- `extra: Option<serde_json::Value>` on `ScrapedDoc` → Task 1
- `extra`/`extractor_name`/`title` on `ScrapeResult` → Task 1
- Scrape→embed path fix → Tasks 2 and 3
- GitHub verticals with `git_*` fields via `build_git_payload` → Task 4
- npm `pkg_*` fields → Task 5
- pypi `pkg_*` + `pypi_requires_python` fields → Task 5
- crates_io `pkg_*` + `crate_*` fields → Task 5
- docs_rs `pkg_*` + `docrs_item_count` → Task 6
- docker_hub `docker_*` fields → Task 7
- huggingface_model `hf_*` fields → Task 7
- dev_to `devto_*` fields → Task 8
- shopify `shop_*` fields → Task 8
- hackernews `hn_*` fields + type inference → Task 9
- stackoverflow `so_*` fields → Task 9
- arxiv `arxiv_*` fields → Task 9
- amazon `amz_*` fields → Task 10
- ebay `ebay_*` fields → Task 10
- Qdrant payload indexes → Task 11
- reddit: **explicitly out of scope** (spec says reddit vertical does NOT emit flat `reddit_*` fields)
- Spec Implementation Status update → Task 12

**Type consistency:**
- `ScrapedDoc.extra: Option<serde_json::Value>` — used as `extra: Some(build_extra(...))` in all extractors
- `ScrapeResult.extra: Option<serde_json::Value>` — populated from `doc.extra` in `services/scrape.rs`
- `PreparedDoc.extra: Option<serde_json::Value>` — already exists; populated from `result.extra` in `scrape_result_to_prepared_doc`
- `pipeline.rs:122-126` merges `doc.extra` flat into Qdrant payload — **unchanged, already correct**

**Absent-beats-null rule:** All `build_extra()` functions use `if !field.is_empty()` guards before inserting optional fields. Empty strings and zero values are handled per-field (e.g., `if downloads > 0` for numeric counts where 0 is meaningless).

**Version bumps:** `hackernews`, `stackoverflow`, `arxiv`, `github_issue`, `github_pr`, and `github_release` bump `extractor_version` from 1 to 2 because they now produce richer payloads. Bumping triggers re-embedding on upgrade, which is correct behavior.
