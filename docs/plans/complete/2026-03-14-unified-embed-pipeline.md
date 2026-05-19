# Unified Embed Pipeline Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consolidate all ingest sources (GitHub, Reddit, YouTube, Sessions) onto the crawl pipeline (`run_embed_pipeline`) — one `PreparedDoc` per logical unit, concurrent per-URL TEI calls — and delete the broken batch machinery.

**Architecture:** Extend `PreparedDoc` with `source_type`, `content_type`, `title`, and `extra` fields, then expose `embed_prepared_docs()` as a `pub(crate)` entry point that all ingest callers use. Each ingest source builds `Vec<PreparedDoc>` (one per file/post/video/session), passing its rich metadata through the same concurrent pipeline that crawl already uses. The batch path (`embed_documents_batch`, `embed_documents_in_batches`, `PreparedBatchDocument`, `EmbedDocument`, `embed_pipeline.rs`) is deleted in full.

**Tech Stack:** Rust, tokio, `FuturesUnordered`, httpmock (tests)

---

## File Structure

**Modified:**
- `crates/vector/ops/tei.rs` — extend `PreparedDoc`, add `embed_prepared_docs`, remove batch code
- `crates/vector/ops/tei/pipeline.rs` — use new PreparedDoc fields in Qdrant payload
- `crates/vector/ops/tei/prepare.rs` — populate new PreparedDoc fields with crawl defaults
- `crates/vector/ops.rs` — update re-exports
- `crates/ingest/github/files.rs` — one PreparedDoc per file, call `embed_prepared_docs`
- `crates/ingest/github.rs` — repo metadata embed → PreparedDoc
- `crates/ingest/github/issues.rs` — issues + PRs → PreparedDoc
- `crates/ingest/github/wiki.rs` — wiki pages → PreparedDoc
- `crates/ingest/reddit.rs` — build PreparedDoc, call `embed_prepared_docs`
- `crates/ingest/youtube.rs` — build Vec<PreparedDoc>, call `embed_prepared_docs`
- `crates/ingest/sessions.rs` — build PreparedDoc, call `embed_prepared_docs`
- `crates/ingest.rs` — remove `pub mod embed_pipeline`

**Deleted:**
- `crates/ingest/embed_pipeline.rs`

---

## Task 1: Extend PreparedDoc and expose unified pipeline entry point

**Files:**
- Modify: `crates/vector/ops/tei.rs`
- Modify: `crates/vector/ops/tei/pipeline.rs`
- Modify: `crates/vector/ops/tei/prepare.rs`
- Modify: `crates/vector/ops.rs`

### Context

`PreparedDoc` is currently `pub(super)` and has only `url`, `domain`, `chunks`. The crawl pipeline's `embed_prepared_doc` in `pipeline.rs` hardcodes `"source_command": "embed"` and `"content_type": "markdown"` in the Qdrant payload — no room for ingest metadata. This task extends the struct and wires the new fields through the payload builder.

- [ ] **Step 1: Write a failing test for PreparedDoc metadata fields**

Add to the existing `#[cfg(test)] mod tests` block in `crates/vector/ops/tei.rs`:

```rust
#[test]
fn prepared_doc_with_ingest_metadata_compiles() {
    // Compile-time check: all four new fields must exist on PreparedDoc.
    // This test FAILS before Step 3 adds them (unknown field errors).
    let doc = PreparedDoc {
        url: "https://github.com/owner/repo/blob/main/src/lib.rs".to_string(),
        domain: "github.com".to_string(),
        chunks: vec!["fn main() {}".to_string()],
        source_type: "github".to_string(),
        content_type: "text",
        title: Some("src/lib.rs".to_string()),
        extra: Some(serde_json::json!({"gh_owner": "owner", "gh_repo": "repo"})),
    };
    assert_eq!(doc.source_type, "github");
    assert_eq!(doc.content_type, "text");
    assert!(doc.title.is_some());
    assert!(doc.extra.is_some());
}
```

- [ ] **Step 2: Run test to confirm it fails (won't compile — fields don't exist yet)**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test prepared_doc_with_ingest_metadata_compiles 2>&1 | head -20
```

Expected: compile error — `PreparedDoc` has no field `source_type`.

- [ ] **Step 3: Extend `PreparedDoc` in `crates/vector/ops/tei.rs`**

Replace the existing `PreparedDoc` definition:

```rust
// OLD — remove:
#[derive(Debug)]
pub(super) struct PreparedDoc {
    url: String,
    domain: String,
    chunks: Vec<String>,
}
```

With:

```rust
// NEW:
#[derive(Debug)]
pub(crate) struct PreparedDoc {
    pub(crate) url: String,
    pub(crate) domain: String,
    pub(crate) chunks: Vec<String>,
    /// "embed" for crawl path, "github"/"reddit"/"youtube"/"sessions" for ingest.
    pub(crate) source_type: String,
    /// "markdown" for crawl path, "text" for ingest sources.
    pub(crate) content_type: &'static str,
    pub(crate) title: Option<String>,
    /// Source-specific metadata fields (gh_*, reddit_*, yt_*).
    pub(crate) extra: Option<serde_json::Value>,
}
```

- [ ] **Step 4: Add `embed_prepared_docs` entry point in `crates/vector/ops/tei.rs`**

Add after the `embed_code_with_metadata` function:

```rust
/// Embed a batch of pre-prepared documents through the unified concurrent pipeline.
///
/// Each `PreparedDoc` must already be chunked. The pipeline processes documents
/// concurrently (AXON_EMBED_DOC_CONCURRENCY), one TEI call per document, and
/// batches Qdrant upserts at 256 points. This is the single entry point for all
/// ingest sources and the crawl path.
pub(crate) async fn embed_prepared_docs(
    cfg: &Config,
    docs: Vec<PreparedDoc>,
    progress_tx: Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
) -> Result<EmbedSummary, Box<dyn Error>> {
    if docs.is_empty() {
        return prepare::emit_empty_embed(progress_tx);
    }
    pipeline::run_embed_pipeline(cfg, docs, progress_tx).await
}
```

- [ ] **Step 5: Update `pipeline.rs` to use the new PreparedDoc fields**

In `crates/vector/ops/tei/pipeline.rs`, inside `embed_prepared_doc`, replace the payload construction block. Find the `serde_json::json!({...})` block that currently hardcodes `"source_command": "embed"` and `"content_type": "markdown"`, and replace it with:

```rust
let mut payload = serde_json::json!({
    "url": doc.url,
    "domain": doc.domain,
    "source_type": doc.source_type,
    "source_command": doc.source_type,
    "content_type": doc.content_type,
    "chunk_index": idx,
    "chunk_text": chunk,
    "scraped_at": timestamp,
});
if let Some(t) = &doc.title {
    payload["title"] = serde_json::Value::String(t.clone());
}
if let Some(serde_json::Value::Object(map)) = &doc.extra {
    for (k, v) in map {
        payload[k] = v.clone();
    }
}
```

> Leave the point_id computation, vectors, and Qdrant upsert logic untouched. Only the payload block changes.

- [ ] **Step 6: Update `prepare.rs` to set crawl defaults on PreparedDoc**

In `crates/vector/ops/tei/prepare.rs`, in `prepare_embed_docs`, update the struct literal (the `prepared.push(PreparedDoc {...})` call):

```rust
// OLD:
prepared.push(PreparedDoc {
    url,
    domain,
    chunks,
});
```

```rust
// NEW:
prepared.push(PreparedDoc {
    url,
    domain,
    chunks,
    source_type: "embed".to_string(),
    content_type: "markdown",
    title: None,
    extra: None,
});
```

- [ ] **Step 7: Update `ops.rs` re-exports**

In `crates/vector/ops.rs`, update the `tei` re-export line to add `embed_prepared_docs`. Keep `EmbedDocument` and `embed_documents_batch` for now — they are deleted in Task 5 after all callers are migrated.

```rust
// OLD:
pub use tei::{
    EmbedDocument, EmbedProgress, EmbedSummary, embed_code_with_metadata, embed_documents_batch,
    embed_path_native, embed_path_native_with_progress, embed_text_with_extra_payload,
    embed_text_with_metadata,
};
```

```rust
// NEW:
pub use tei::{
    EmbedDocument, EmbedProgress, EmbedSummary, embed_code_with_metadata, embed_documents_batch,
    embed_path_native, embed_path_native_with_progress, embed_text_with_extra_payload,
    embed_text_with_metadata,
};
pub(crate) use tei::{PreparedDoc, embed_prepared_docs};
```

- [ ] **Step 8: Run tests**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test prepared_doc_with_ingest_metadata_compiles -- --nocapture
```

Expected: PASS.

- [ ] **Step 9: Full compile check**

```bash
cargo check 2>&1 | grep "^error" | head -20
```

Expected: 0 errors.

- [ ] **Step 10: Commit**

```bash
git add crates/vector/ops/tei.rs crates/vector/ops/tei/pipeline.rs crates/vector/ops/tei/prepare.rs crates/vector/ops.rs
git commit -m "feat(embed): extend PreparedDoc with metadata fields, expose embed_prepared_docs entry point"
```

---

## Task 2: Migrate GitHub files ingest

**Files:**
- Modify: `crates/ingest/github/files.rs`

### Context

After a previous bad commit, `read_file_embed_doc` returns `Vec<EmbedDocument>` (one per chunk). The correct model is **one PreparedDoc per file** with all chunks in `Vec<String>`, matching the crawl pipeline. This task reverts and replaces that with the correct design.

- [ ] **Step 1: Update imports**

Replace the imports block at the top of `crates/ingest/github/files.rs`. The key changes:
- Remove: `use crate::crates::ingest::embed_pipeline::embed_documents_in_batches;`
- Remove: `use crate::crates::vector::ops::{EmbedDocument, embed_code_with_metadata};`
- Add: `use crate::crates::vector::ops::{EmbedProgress, EmbedSummary, PreparedDoc, embed_prepared_docs};`
- Keep: `use crate::crates::vector::ops::input::{chunk_text, code::chunk_code};` (both are still needed)
- Remove: `use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};` (no longer needed)

Full corrected import block:

```rust
use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::vector::ops::input::classify::{
    classify_file_type, is_test_path, language_name,
};
use crate::crates::vector::ops::input::{chunk_text, code::chunk_code};
use crate::crates::vector::ops::{EmbedProgress, EmbedSummary, PreparedDoc, embed_prepared_docs};
use futures_util::stream::{self, StreamExt};
use std::error::Error;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use super::meta::{GitHubPayloadParams, build_github_payload};
use super::{GitHubCommonFields, is_indexable_doc_path, is_indexable_source_path};
```

Also remove the constant:
```rust
// REMOVE:
const GITHUB_EMBED_DOC_BATCH_SIZE: usize = 256;
```

- [ ] **Step 2: Replace `read_file_embed_doc` with one-PreparedDoc-per-file**

Replace the entire `read_file_embed_doc` function:

```rust
/// Read a single file from the cloned repo and build a PreparedDoc with all chunks.
///
/// Returns one `PreparedDoc` per file — all chunks for the file are in
/// `PreparedDoc.chunks`. The unified pipeline issues one TEI call per PreparedDoc.
/// Empty or unreadable files return `Ok(None)`.
async fn read_file_embed_doc(ctx: &FileEmbedCtx, path: &str) -> Result<Option<PreparedDoc>, String> {
    let full_path = ctx.repo_root.join(path);
    let text = match tokio::fs::read_to_string(&full_path).await {
        Ok(t) => t,
        Err(e) => {
            log_warn(&format!(
                "command=ingest_github read_failed path={path} err={e}"
            ));
            return Ok(None);
        }
    };
    if text.trim().is_empty() {
        return Ok(None);
    }

    let ext = file_extension(path);
    let chunks = chunk_code(&text, &ext).unwrap_or_else(|| chunk_text(&text));
    if chunks.is_empty() {
        return Ok(None);
    }

    let extra = build_github_payload(&GitHubPayloadParams {
        repo: ctx.name.clone(),
        owner: ctx.owner.clone(),
        content_kind: "file".into(),
        branch: Some(ctx.default_branch.clone()),
        default_branch: Some(ctx.default_branch.clone()),
        repo_description: ctx.repo_description.clone(),
        pushed_at: ctx.pushed_at.clone(),
        is_private: ctx.is_private,
        file_path: Some(path.to_string()),
        file_language: Some(language_name(&ext).to_string()),
        file_type: Some(classify_file_type(path).to_string()),
        is_test: Some(is_test_path(path)),
        file_size_bytes: Some(text.len()),
        chunking_method: Some(chunking_method(&ext).to_string()),
        ..Default::default()
    });

    let source_url = format!(
        "https://github.com/{}/{}/blob/{}/{}",
        ctx.owner, ctx.name, ctx.default_branch, path
    );

    Ok(Some(PreparedDoc {
        url: source_url,
        domain: "github.com".to_string(),
        chunks,
        source_type: "github".to_string(),
        content_type: "text",
        title: Some(path.to_string()),
        extra: Some(extra),
    }))
}
```

- [ ] **Step 3: Update `collect_embed_docs` to return `Vec<PreparedDoc>`**

Replace the function signature and match arm:

```rust
async fn collect_embed_docs(
    ctx: &FileEmbedCtx,
    file_items: Vec<String>,
    files_total: usize,
    progress_tx: Option<&mpsc::Sender<serde_json::Value>>,
    failed: &mut usize,
) -> Vec<PreparedDoc> {
    let concurrency = std::cmp::min(ctx.cfg.batch_concurrency, 64);
    let mut file_stream = stream::iter(file_items)
        .map(|path| {
            let ctx = ctx.clone();
            async move { read_file_embed_doc(&ctx, &path).await }
        })
        .buffer_unordered(concurrency);

    let mut docs: Vec<PreparedDoc> = Vec::new();
    let mut files_done = 0usize;

    while let Some(result) = file_stream.next().await {
        files_done += 1;
        match result {
            Ok(Some(doc)) => docs.push(doc),
            Ok(None) => {}
            Err(_) => *failed += 1,
        }
        if files_done.is_multiple_of(FILE_PROGRESS_EVERY) || files_done == files_total {
            send_progress(
                progress_tx,
                serde_json::json!({
                    "files_done": files_done,
                    "files_total": files_total,
                    "chunks_embedded": 0,
                    "phase": "collecting_files",
                }),
            )
            .await;
        }
    }

    docs
}
```

- [ ] **Step 4: Delete `embed_collected_docs` and update `embed_files`**

Delete the entire `embed_collected_docs` function. Replace `embed_files` with:

```rust
pub async fn embed_files(
    cfg: &Config,
    common: &GitHubCommonFields,
    include_source: bool,
    token: Option<&str>,
    progress_tx: Option<&mpsc::Sender<serde_json::Value>>,
) -> Result<usize, Box<dyn Error>> {
    let tmp = clone_repo(common, &common.default_branch, token).await?;
    let repo_root = tmp.path().to_path_buf();
    let file_items = collect_indexable_files(&repo_root, include_source).await?;
    let files_total = file_items.len();

    log_info(&format!(
        "github clone complete indexable={files_total} repo={}",
        common.repo_slug
    ));

    let ctx = FileEmbedCtx {
        cfg: cfg.clone(),
        repo_root,
        owner: common.owner.clone(),
        name: common.name.clone(),
        default_branch: common.default_branch.clone(),
        repo_description: common.repo_description.clone(),
        pushed_at: common.pushed_at.clone(),
        is_private: common.is_private,
    };
    let mut failed = 0usize;
    let docs = collect_embed_docs(&ctx, file_items, files_total, progress_tx, &mut failed).await;

    send_progress(
        progress_tx,
        serde_json::json!({
            "files_done": files_total,
            "files_total": files_total,
            "chunks_embedded": 0,
            "phase": "embedding",
        }),
    )
    .await;

    let summary = embed_prepared_docs(cfg, docs, None).await?;
    let chunks_embedded = summary.chunks_embedded;

    send_progress(
        progress_tx,
        serde_json::json!({
            "files_done": files_total,
            "files_total": files_total,
            "chunks_embedded": chunks_embedded,
            "phase": "embedded_files",
        }),
    )
    .await;

    log_info(&format!(
        "github files_embedded total={files_total} failed={failed} chunks={chunks_embedded}"
    ));
    Ok(chunks_embedded)
}
```

> The `EmbedProgress` and `EmbedSummary` imports added in Step 1 are used implicitly through `embed_prepared_docs` return type — keep them.

- [ ] **Step 5: Compile check**

```bash
cd /home/jmagar/workspace/axon_rust
cargo check -p axon 2>&1 | grep "^error" | head -20
```

Expected: 0 errors in `crates/ingest/github/files.rs`.

- [ ] **Step 6: Run ingest tests**

```bash
cargo test chunk_code -- --nocapture
cargo test chunk_text -- --nocapture
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/ingest/github/files.rs
git commit -m "refactor(ingest/github): one PreparedDoc per file via unified embed pipeline"
```

---

## Task 2b: Migrate GitHub issues, PRs, wiki, and repo metadata embeds

**Files:**
- Modify: `crates/ingest/github.rs`
- Modify: `crates/ingest/github/issues.rs`
- Modify: `crates/ingest/github/wiki.rs`

### Context

Three more GitHub ingest files use `EmbedDocument` + `embed_documents_in_batches` and will block Task 5's deletion if not migrated. These handle:
- `github.rs`: repo-level metadata (README + description) — one document per repo
- `github/issues.rs`: one EmbedDocument per issue and one per PR — collected in a Vec, then batch-embedded
- `github/wiki.rs`: one EmbedDocument per wiki page — collected in a Vec, then batch-embedded

All three use prose text content → `chunk_text` is the correct chunker.

### github.rs — repo metadata

- [ ] **Step 1: Update imports in `crates/ingest/github.rs`**

```rust
// REMOVE:
use crate::crates::ingest::embed_pipeline::embed_documents_in_batches;
use crate::crates::vector::ops::{EmbedDocument, embed_text_with_extra_payload};

// ADD:
use crate::crates::vector::ops::{PreparedDoc, embed_prepared_docs};
use crate::crates::vector::ops::input::chunk_text;
```

- [ ] **Step 2: Replace the repo metadata embed block in `github.rs`**

Find the block that builds `docs = vec![EmbedDocument {...}]` and calls `embed_documents_in_batches` (around lines 157–188). Replace it with:

```rust
let chunks = chunk_text(&content);
if chunks.is_empty() {
    return Ok(0);
}
let domain = spider::url::Url::parse(&url)
    .ok()
    .and_then(|u| u.host_str().map(|s| s.to_string()))
    .unwrap_or_else(|| "github.com".to_string());
let doc = PreparedDoc {
    url,
    domain,
    chunks,
    source_type: "github".to_string(),
    content_type: "text",
    title: Some(owner_name.to_string()),
    extra: Some(extra),
};
let summary = embed_prepared_docs(cfg, vec![doc], None).await?;
Ok(summary.chunks_embedded)
```

### github/issues.rs — issues and PRs

- [ ] **Step 3: Update imports in `crates/ingest/github/issues.rs`**

```rust
// REMOVE:
use crate::crates::ingest::embed_pipeline::embed_documents_in_batches;
use crate::crates::vector::ops::{EmbedDocument, embed_text_with_extra_payload};

// ADD:
use crate::crates::vector::ops::{PreparedDoc, embed_prepared_docs};
use crate::crates::vector::ops::input::chunk_text;
```

- [ ] **Step 4: Change the `docs` Vec type in `ingest_issues` and `ingest_pull_requests`**

In both functions, change:
```rust
// OLD:
let mut docs = Vec::new();
// ...
docs.push(EmbedDocument { content, url, source_type: "github".to_string(), title: Some(title), extra: Some(extra), file_extension: None });
```

To:
```rust
// NEW:
let mut docs: Vec<PreparedDoc> = Vec::new();
// ...
let chunks = chunk_text(&content);
if !chunks.is_empty() {
    let domain = spider::url::Url::parse(&url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "github.com".to_string());
    docs.push(PreparedDoc {
        url,
        domain,
        chunks,
        source_type: "github".to_string(),
        content_type: "text",
        title: Some(title),
        extra: Some(extra),
    });
}
```

Apply this pattern to both the issue push (around line 72) and the PR push (around line 146).

- [ ] **Step 5: Replace `embed_github_docs` with a direct `embed_prepared_docs` call**

Delete `embed_github_docs`. Replace `Ok(embed_github_docs(cfg, &docs, "ingest_github").await)` at the end of both `ingest_issues` and `ingest_pull_requests` with:

```rust
let summary = embed_prepared_docs(cfg, docs, None).await?;
Ok(summary.chunks_embedded)
```

### github/wiki.rs — wiki pages

- [ ] **Step 6: Update imports in `crates/ingest/github/wiki.rs`**

```rust
// REMOVE:
use crate::crates::ingest::embed_pipeline::embed_documents_in_batches;
use crate::crates::vector::ops::{EmbedDocument, embed_text_with_extra_payload};

// ADD:
use crate::crates::vector::ops::{PreparedDoc, embed_prepared_docs};
use crate::crates::vector::ops::input::chunk_text;
```

- [ ] **Step 7: Replace `docs: Vec<EmbedDocument>` with `Vec<PreparedDoc>` in the wiki embed block**

Find where `docs.push(EmbedDocument {...})` is called (around line 133) and the subsequent `embed_documents_in_batches` call (around line 143). Replace the entire section:

```rust
// OLD:
docs.push(EmbedDocument {
    content,
    url: wiki_url,
    source_type: "github".to_string(),
    title: Some(title),
    extra: Some(extra),
    file_extension: None,
});
// ... later ...
let result = embed_documents_in_batches(cfg, &docs, 64, "ingest_github", |cfg, doc| { ... }, |_| {}).await;
Ok(result.chunks_embedded)
```

With:

```rust
// NEW (update the docs Vec type declaration at the start of the function too):
let mut docs: Vec<PreparedDoc> = Vec::new();
// ... in the loop:
let chunks = chunk_text(&content);
if !chunks.is_empty() {
    let domain = spider::url::Url::parse(&wiki_url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "github.com".to_string());
    docs.push(PreparedDoc {
        url: wiki_url,
        domain,
        chunks,
        source_type: "github".to_string(),
        content_type: "text",
        title: Some(title),
        extra: Some(extra),
    });
}
// ... after the loop:
let summary = embed_prepared_docs(cfg, docs, None).await?;
Ok(summary.chunks_embedded)
```

- [ ] **Step 8: Compile check**

```bash
cd /home/jmagar/workspace/axon_rust
cargo check -p axon 2>&1 | grep "^error" | head -20
```

Expected: 0 errors across all three github files.

- [ ] **Step 9: Commit**

```bash
git add crates/ingest/github.rs crates/ingest/github/issues.rs crates/ingest/github/wiki.rs
git commit -m "refactor(ingest/github): migrate issues, PRs, wiki, and metadata embeds to unified pipeline"
```

---

## Task 3: Migrate Reddit and YouTube ingest

**Files:**
- Modify: `crates/ingest/reddit.rs`
- Modify: `crates/ingest/youtube.rs`

### Reddit

- [ ] **Step 1: Update imports in `crates/ingest/reddit.rs`**

```rust
// REMOVE:
use crate::crates::ingest::embed_pipeline::embed_documents_in_batches;
use crate::crates::vector::ops::{EmbedDocument, embed_text_with_extra_payload};

// ADD:
use crate::crates::vector::ops::{PreparedDoc, embed_prepared_docs};
use crate::crates::vector::ops::input::chunk_text;
```

- [ ] **Step 2: Replace the `ingest_subreddit` embed block**

Find the block that builds `EmbedDocument` and calls `embed_reddit_documents` (around lines 100–110). Replace with:

```rust
let chunks = chunk_text(&content);
if !chunks.is_empty() {
    let domain = spider::url::Url::parse(&post_url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "reddit.com".to_string());
    let doc = PreparedDoc {
        url: post_url.clone(),
        domain,
        chunks,
        source_type: "reddit".to_string(),
        content_type: "text",
        title: Some(title.to_string()),
        extra: Some(extra.clone()),
    };
    if let Ok(summary) = embed_prepared_docs(cfg, vec![doc], None).await {
        count_ref.fetch_add(summary.chunks_embedded, Ordering::SeqCst);
    }
}
```

- [ ] **Step 3: Replace the `ingest_thread` embed block**

Find the block that builds `EmbedDocument` and calls `embed_reddit_documents` (around lines 172–181). Replace with:

```rust
let chunks = chunk_text(&content);
if chunks.is_empty() {
    return Ok(0);
}
let domain = spider::url::Url::parse(&canonical_url)
    .ok()
    .and_then(|u| u.host_str().map(|s| s.to_string()))
    .unwrap_or_else(|| "reddit.com".to_string());
let doc = PreparedDoc {
    url: canonical_url,
    domain,
    chunks,
    source_type: "reddit".to_string(),
    content_type: "text",
    title: Some(title.to_string()),
    extra: Some(extra.clone()),
};
let summary = embed_prepared_docs(cfg, vec![doc], None).await?;
Ok(summary.chunks_embedded)
```

- [ ] **Step 4: Delete `embed_reddit_documents`**

Delete the entire `embed_reddit_documents` async function.

### YouTube

- [ ] **Step 5: Update imports in `crates/ingest/youtube.rs`**

```rust
// REMOVE:
use crate::crates::ingest::embed_pipeline::embed_documents_in_batches;
use crate::crates::vector::ops::{EmbedDocument, embed_text_with_extra_payload, embed_text_with_metadata};

// ADD:
use crate::crates::vector::ops::{PreparedDoc, embed_prepared_docs};
use crate::crates::vector::ops::input::chunk_text;
```

- [ ] **Step 6: Replace the YouTube embed block**

Find where `docs = vec![EmbedDocument {...}]` is built and `embed_youtube_documents` is called (around lines 280–305). Replace with:

```rust
let mut docs: Vec<PreparedDoc> = Vec::new();

let transcript_chunks = chunk_text(&text);
if !transcript_chunks.is_empty() {
    let domain = spider::url::Url::parse(&source_url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "youtube.com".to_string());
    docs.push(PreparedDoc {
        url: source_url.clone(),
        domain,
        chunks: transcript_chunks,
        source_type: "youtube".to_string(),
        content_type: "text",
        title: Some(title.to_string()),
        extra: extra.clone(),
    });
}

if let Some(m) = &video_meta
    && !m.description.trim().is_empty()
{
    let desc_url = format!("{source_url}?section=description");
    let desc_chunks = chunk_text(&m.description);
    if !desc_chunks.is_empty() {
        let domain = spider::url::Url::parse(&desc_url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "youtube.com".to_string());
        docs.push(PreparedDoc {
            url: desc_url,
            domain,
            chunks: desc_chunks,
            source_type: "youtube".to_string(),
            content_type: "text",
            title: Some(format!("{} — description", m.title)),
            extra: extra.clone(),
        });
    }
}

if !docs.is_empty() {
    if let Ok(summary) = embed_prepared_docs(cfg, docs, None).await {
        count += summary.chunks_embedded;
    }
}
```

- [ ] **Step 7: Delete `embed_youtube_documents`**

Delete the entire `embed_youtube_documents` async function.

- [ ] **Step 8: Compile check**

```bash
cd /home/jmagar/workspace/axon_rust
cargo check -p axon 2>&1 | grep "^error" | head -20
```

Expected: 0 errors in reddit.rs and youtube.rs.

- [ ] **Step 9: Run ingest tests**

```bash
cargo test parse_vtt -- --nocapture
cargo test extract_video -- --nocapture
cargo test classify -- --nocapture
```

Expected: all pass.

- [ ] **Step 10: Commit**

```bash
git add crates/ingest/reddit.rs crates/ingest/youtube.rs
git commit -m "refactor(ingest/reddit,youtube): migrate to unified PreparedDoc embed pipeline"
```

---

## Task 4: Migrate Sessions ingest

**Files:**
- Modify: `crates/ingest/sessions.rs`

### Context

`embed_session_text` wraps a single text blob in the entire batch machinery. Replace with a direct `PreparedDoc` + `embed_prepared_docs` call.

- [ ] **Step 1: Update imports in `crates/ingest/sessions.rs`**

```rust
// REMOVE:
use crate::crates::vector::ops::{EmbedDocument, embed_text_with_metadata};
// REMOVE (if present):
use crate::crates::ingest::embed_pipeline::embed_documents_in_batches;

// ADD:
use crate::crates::vector::ops::{PreparedDoc, embed_prepared_docs};
use crate::crates::vector::ops::input::chunk_text;
```

- [ ] **Step 2: Replace `embed_session_text`**

Replace the entire function body:

```rust
pub(crate) async fn embed_session_text(
    cfg: &Config,
    session_text: String,
    url: String,
    source_type: &str,
    title: Option<&str>,
) -> IngestResult<usize> {
    if session_text.trim().is_empty() {
        return Ok(0);
    }

    let chunks = chunk_text(&session_text);
    if chunks.is_empty() {
        return Ok(0);
    }

    let domain = spider::url::Url::parse(&url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "local".to_string());

    let doc = PreparedDoc {
        url,
        domain,
        chunks,
        source_type: source_type.to_string(),
        content_type: "text",
        title: title.map(str::to_string),
        extra: None,
    };

    let summary = embed_prepared_docs(cfg, vec![doc], None)
        .await
        .map_err(|e| anyhow::anyhow!("embed_session_text failed: {e}"))?;

    Ok(summary.chunks_embedded)
}
```

- [ ] **Step 3: Compile check**

```bash
cd /home/jmagar/workspace/axon_rust
cargo check -p axon 2>&1 | grep "^error" | head -20
```

Expected: 0 errors.

- [ ] **Step 4: Run session tests**

```bash
cargo test session -- --nocapture
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/ingest/sessions.rs
git commit -m "refactor(ingest/sessions): migrate embed_session_text to unified PreparedDoc pipeline"
```

---

## [VERIFY] Pre-deletion checkpoint

Before deleting any code, confirm all callers are migrated and the full codebase compiles clean.

- [ ] **Step 1: Confirm no remaining EmbedDocument / batch callers**

```bash
grep -rn "EmbedDocument\|embed_documents_in_batches\|embed_documents_batch" \
  /home/jmagar/workspace/axon_rust/crates --include="*.rs" \
  | grep -v "tei.rs\|ops.rs"
```

Expected: 0 results. If any appear, migrate that file before proceeding to Task 5.

- [ ] **Step 2: Full test suite**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test -- --nocapture 2>&1 | tail -20
```

Expected: 0 failures.

- [ ] **Step 3: Clippy**

```bash
cargo clippy -- -D warnings 2>&1 | grep "^error" | head -20
```

Expected: 0 warnings promoted to errors. (Unused import warnings for `EmbedDocument`/`embed_documents_batch` in `tei.rs` and `ops.rs` are fine — deleted in Task 5.)

---

## Task 5: Delete dead code

**Files:**
- Modify: `crates/vector/ops/tei.rs` — remove batch machinery
- Modify: `crates/vector/ops.rs` — update re-exports
- Modify: `crates/ingest.rs` — remove `pub mod embed_pipeline`
- Delete: `crates/ingest/embed_pipeline.rs`

### Context

With all callers migrated, the following are now unused. Delete by name — do not use line numbers (they shifted from Task 1 edits).

- [ ] **Step 1: Remove the batch machinery from `crates/vector/ops/tei.rs`**

Delete the following named items from `tei.rs` (in the order they appear):

1. `EmbedDocument` struct — the `pub struct EmbedDocument { ... }` block
2. `PreparedBatchDocument` struct — the `struct PreparedBatchDocument { ... }` block
3. `prepare_batch_document` fn — `fn prepare_batch_document(doc: &EmbedDocument) -> Option<PreparedBatchDocument>`
4. `validate_batch_vectors` fn — `fn validate_batch_vectors(...)`
5. `build_batch_points` fn — `fn build_batch_points(...)`
6. `cleanup_batch_stale_tails` fn — `async fn cleanup_batch_stale_tails(...)`
7. `embed_documents_batch` fn — `pub async fn embed_documents_batch(...)`

After deletion, `tei.rs` should contain only:
- Imports and submodule declarations
- `EmbedSummary`, `EmbedProgress` structs
- `PreparedDoc` struct
- `embed_text_impl`, `embed_chunks_impl` private fns
- `embed_text_with_metadata`, `embed_text_with_extra_payload`, `embed_code_with_metadata` pub fns
- `embed_prepared_docs` pub(crate) fn
- `embed_path_native`, `embed_path_native_with_progress` pub fns
- `#[cfg(test)] mod tests`

- [ ] **Step 2: Update `crates/vector/ops.rs` re-exports**

Remove `EmbedDocument` and `embed_documents_batch` from the pub re-export. Final state:

```rust
pub use tei::{
    EmbedProgress, EmbedSummary, embed_code_with_metadata,
    embed_path_native, embed_path_native_with_progress, embed_text_with_extra_payload,
    embed_text_with_metadata,
};
pub(crate) use tei::{PreparedDoc, embed_prepared_docs};
```

- [ ] **Step 3: Delete `crates/ingest/embed_pipeline.rs`**

```bash
git rm /home/jmagar/workspace/axon_rust/crates/ingest/embed_pipeline.rs
```

- [ ] **Step 4: Remove `pub mod embed_pipeline` from `crates/ingest.rs`**

Edit `crates/ingest.rs` and remove:

```rust
pub mod embed_pipeline;
```

- [ ] **Step 5: Full compile check**

```bash
cd /home/jmagar/workspace/axon_rust
cargo check -p axon 2>&1 | grep "^error"
```

Expected: 0 errors.

- [ ] **Step 6: Run full test suite**

```bash
cargo test -- --nocapture 2>&1 | tail -20
```

Expected: all tests pass, 0 failures.

- [ ] **Step 7: Clippy clean**

```bash
cargo clippy -- -D warnings 2>&1 | grep "^error" | head -20
```

Expected: 0 errors.

- [ ] **Step 8: Monolith check**

```bash
./scripts/enforce_monoliths.py 2>&1 | grep -E "FAIL|ERROR" | head -20
```

Expected: 0 violations.

- [ ] **Step 9: Commit**

```bash
git add crates/vector/ops/tei.rs crates/vector/ops.rs crates/ingest.rs
git commit -m "refactor(embed): delete batch pipeline — EmbedDocument, embed_documents_batch, embed_pipeline.rs"
```

---

## Final Verification

```bash
# Confirm no remaining callers of the deleted batch path
grep -rn "embed_documents_batch\|embed_documents_in_batches\|EmbedDocument\|PreparedBatchDocument" \
  /home/jmagar/workspace/axon_rust/crates --include="*.rs"
# Expected: 0 results

# Confirm all embed callers use the unified pipeline
grep -rn "embed_prepared_docs" \
  /home/jmagar/workspace/axon_rust/crates --include="*.rs"
# Expected callers: github/files.rs, github.rs, github/issues.rs, github/wiki.rs,
#                   reddit.rs, youtube.rs, sessions.rs
# Expected definition: vector/ops/tei.rs
```
