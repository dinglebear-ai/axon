# GitHub Ingest Cleanup Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Address all code review findings in the `crates/ingest/github/` module — two monolith hard-limit violations, silent error swallows, redundant allocations, and several minor cleanups.

**Architecture:** Each task is self-contained and touches one or two files. Tasks are ordered from blocking (must-fix) to informational. The anyhow migration (Task 4) is a pure find-replace across 4 files with no logic change.

**Tech Stack:** Rust 2021, tokio, octocrab, anyhow (already in Cargo.toml)

---

## Files Touched

| File | Tasks |
|------|-------|
| `crates/ingest/github/files.rs` | 1, 4, 5, 7 |
| `crates/ingest/github/wiki.rs` | 2, 4, 5, 6 |
| `crates/ingest/github.rs` | 3, 4, 5 |
| `crates/ingest/github/issues.rs` | 4, 5 |

---

## Chunk 1: Blocking Fixes

### Task 1: `files.rs` — stale docstring + silent `create_dir_all`

**Files:**
- Modify: `crates/ingest/github/files.rs:37-89`

Two independent bugs in `clone_repo`:
1. Lines 37–40 are a stale first fragment of the doc comment that contradicts and duplicates lines 41–46. Delete it.
2. Line 89: `let _ = tokio::fs::create_dir_all(tmp.path()).await;` — if this fails, the unauthenticated `git clone` attempts a write into a non-existent path and produces a confusing "No such file or directory" error. Propagate it.

- [ ] **Step 1: Delete stale docstring fragment**

In `files.rs`, delete lines 37–40:
```
/// Clone the repo with `git clone --depth=1` into a temp directory.
///
/// Returns the temp directory handle (dropped = cleanup) and the path.
/// Auth uses `http.extraHeader` via git config env vars — token never appears in process args.
```
Keep lines 41–46 (the accurate doc comment starting with `/// Run git clone --depth=1...`).

- [ ] **Step 2: Propagate `create_dir_all` error**

Replace line 89:
```rust
        let _ = tokio::fs::create_dir_all(tmp.path()).await;
```
With:
```rust
        tokio::fs::create_dir_all(tmp.path())
            .await
            .map_err(|e| format!("failed to recreate tmp dir for unauthenticated retry: {e}"))?;
```

- [ ] **Step 3: Verify compile**

```bash
cargo check --bin axon 2>&1 | grep "^error"
```
Expected: no output

- [ ] **Step 4: Run existing tests**

```bash
cargo test -p axon_cli ingest 2>&1 | tail -5
```
Expected: all pass

- [ ] **Step 5: Commit**

```bash
git add crates/ingest/github/files.rs
git commit -m "fix(ingest): propagate create_dir_all error in clone retry + remove stale docstring"
```

---

### Task 2: `wiki.rs` — add log_warn to credential bypass + extract `build_wiki_docs`

**Files:**
- Modify: `crates/ingest/github/wiki.rs`

Two changes:
1. The `invalid credentials` silent bypass (lines 75–79) needs a `log_warn` so failures are auditable in logs. Right now a real auth failure on a wiki that exists would silently return 0 chunks with no trace.
2. `ingest_wiki` is 115 lines (over 80-line warning). Extract the directory-walk + doc-building block into `build_wiki_docs`.

- [ ] **Step 1: Add log_warn to the invalid-credentials branch**

Replace the `invalid credentials` arm (currently inside the combined condition) with a separate, logged branch. Change:
```rust
        if stderr.contains("not found")
            || stderr.contains("does not exist")
            || (token.is_some() && stderr.contains("invalid credentials"))
        {
            return Ok(0);
        }
```
To:
```rust
        if stderr.contains("not found") || stderr.contains("does not exist") {
            return Ok(0);
        }
        // GitHub returns "invalid credentials" (not "not found") when a valid token
        // is provided but the wiki repo doesn't exist — anti-enumeration behaviour.
        // Log it so a genuine auth failure on an existing wiki is still visible.
        if token.is_some() && stderr.contains("invalid credentials") {
            log_warn(&format!(
                "command=ingest_github wiki_no_credentials repo={}/{} \
                 treating_as_no_wiki (GitHub anti-enumeration)",
                common.owner, common.name
            ));
            return Ok(0);
        }
```

- [ ] **Step 2: Extract `build_wiki_docs`**

Add this new private async function above `ingest_wiki` (after the `walk_dir_recursive` fn):
```rust
/// Build PreparedDoc list from the files in a cloned wiki directory.
async fn build_wiki_docs(
    tmp_path: &str,
    common: &GitHubCommonFields,
) -> Result<Vec<PreparedDoc>, Box<dyn Error>> {
    let all_files = walk_dir_recursive(Path::new(tmp_path)).await?;
    let mut docs: Vec<PreparedDoc> = Vec::new();

    for path in all_files {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if !matches!(ext.as_str(), "md" | "rst" | "txt") {
            continue;
        }

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                log_warn(&format!(
                    "command=ingest_github wiki_read_failed path={path:?} err={e}"
                ));
                continue;
            }
        };

        if content.trim().is_empty() {
            continue;
        }

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Home");
        let wiki_url = format!(
            "https://github.com/{}/{}/wiki/{stem}",
            common.owner, common.name
        );
        let title = stem.replace(['-', '_'], " ");

        let extra = build_github_payload(&GitHubPayloadParams {
            repo: common.name.clone(),
            owner: common.owner.clone(),
            content_kind: "wiki".into(),
            default_branch: Some(common.default_branch.clone()),
            repo_description: common.repo_description.clone(),
            pushed_at: common.pushed_at.clone(),
            is_private: common.is_private,
            ..Default::default()
        });

        let chunks = chunk_text(&content);
        if !chunks.is_empty() {
            docs.push(PreparedDoc {
                url: wiki_url,
                domain: "github.com".to_string(),
                chunks,
                source_type: "github".to_string(),
                content_type: "text",
                title: Some(title),
                extra: Some(extra),
            });
        }
    }

    Ok(docs)
}
```

- [ ] **Step 3: Replace the walk+build block in `ingest_wiki` with a call**

Remove lines 85–148 from `ingest_wiki` (the walk + for loop). Replace them with:
```rust
    let docs = build_wiki_docs(&tmp_path, common).await?;
    let summary = embed_prepared_docs(cfg, docs, None).await?;
    Ok(summary.chunks_embedded)
```

The `spider::url::Url::parse` block that was in the loop is now gone; also remove the `spider::url::Url` usage at line 134 since it's replaced by the hardcoded `"github.com".to_string()` in the new function.

- [ ] **Step 4: Verify compile + line counts**

```bash
cargo check --bin axon 2>&1 | grep "^error"
wc -l crates/ingest/github/wiki.rs
```
Expected: no errors; `ingest_wiki` function body should be under 60 lines

- [ ] **Step 5: Run tests**

```bash
cargo test -p axon_cli ingest 2>&1 | tail -5
```
Expected: all pass

- [ ] **Step 6: Commit**

```bash
git add crates/ingest/github/wiki.rs
git commit -m "fix(ingest): log wiki credential bypass + extract build_wiki_docs (monolith fix)"
```

---

### Task 3: `github.rs` — fix progress sender + extract `tally_results`

**Files:**
- Modify: `crates/ingest/github.rs`

Two changes:
1. The nested `send_ingest_progress` fn takes `&Option<Sender>` and every call site wraps `tx.clone()` into `Some(...)` redundantly. Replace with a module-level `send_progress` fn matching `files.rs` idiom: `Option<&Sender>`.
2. Extract the result-aggregation loop + final progress send into `tally_results` to bring `ingest_github` under the 80-line warning threshold (currently 124 lines).

- [ ] **Step 1: Add `use tokio::sync::mpsc` import**

Near the top of `github.rs`, add:
```rust
use tokio::sync::mpsc;
```

- [ ] **Step 2: Add module-level `send_progress` function**

Add this before `ingest_github` (after `embed_repo_metadata`):
```rust
async fn send_progress(
    tx: Option<&mpsc::Sender<serde_json::Value>>,
    progress: serde_json::Value,
) {
    if let Some(tx) = tx
        && let Err(err) = tx.send(progress).await
    {
        log_warn(&format!(
            "command=ingest_github progress_send_failed err={err}"
        ));
    }
}
```

- [ ] **Step 3: Add `tally_results` function**

Add this before `ingest_github`:
```rust
fn tally_results(
    results: [(&str, Result<usize, Box<dyn Error>>); 5],
    repo: &str,
) -> (usize, usize, usize) {
    let mut total = 0usize;
    let mut issues_count = 0usize;
    let mut prs_count = 0usize;
    for (label, result) in results {
        match result {
            Ok(n) => {
                if label == "issues" {
                    issues_count = n;
                } else if label == "prs" {
                    prs_count = n;
                }
                total += n;
            }
            Err(e) => log_warn(&format!(
                "command=ingest_github {label}_failed repo={repo} err={e}"
            )),
        }
    }
    (total, issues_count, prs_count)
}
```

- [ ] **Step 4: Rewrite `ingest_github` to use the new helpers**

Replace the body of `ingest_github` from line 197 to line 314 with:
```rust
    log_info(&format!("command=ingest source=github repo={repo}"));
    let (owner, name) =
        parse_github_repo(repo).ok_or_else(|| format!("invalid GitHub repo: {repo}"))?;

    let octo = build_octocrab(cfg)?;
    let repo_info = octo.repos(&owner, &name).get().await?;
    let default_branch = repo_info
        .default_branch
        .as_deref()
        .unwrap_or("main")
        .to_string();

    let common = GitHubCommonFields {
        repo_slug: format!("{owner}/{name}"),
        owner: owner.clone(),
        name: name.clone(),
        default_branch: default_branch.clone(),
        repo_description: repo_info.description.clone(),
        pushed_at: repo_info.pushed_at.map(|dt| dt.to_rfc3339()),
        is_private: repo_info.private,
        has_wiki: repo_info.has_wiki.unwrap_or(false),
    };

    send_progress(
        progress_tx.as_ref(),
        serde_json::json!({
            "phase": "ingesting",
            "tasks_total": 5,
            "tasks_done": 0,
        }),
    )
    .await;

    let (files_result, metadata_result, issues_result, prs_result, wiki_result) = tokio::join!(
        files::embed_files(cfg, &common, include_source, cfg.github_token.as_deref(), progress_tx.as_ref()),
        embed_repo_metadata(cfg, &repo_info, &common),
        issues::ingest_issues(cfg, &octo, &common),
        issues::ingest_pull_requests(cfg, &octo, &common),
        async {
            if common.has_wiki {
                wiki::ingest_wiki(cfg, &common, cfg.github_token.as_deref()).await
            } else {
                Ok(0)
            }
        },
    );

    let (total, issues_count, prs_count) = tally_results(
        [
            ("files", files_result),
            ("metadata", metadata_result),
            ("issues", issues_result),
            ("prs", prs_result),
            ("wiki", wiki_result),
        ],
        repo,
    );

    send_progress(
        progress_tx.as_ref(),
        serde_json::json!({
            "tasks_done": 5,
            "tasks_total": 5,
            "chunks_embedded": total,
            "phase": "completed",
        }),
    )
    .await;

    log_info(&format!(
        "github issues_fetched={issues_count} prs_fetched={prs_count}"
    ));
    log_done(&format!(
        "command=ingest source=github repo={repo} chunk_count={total}"
    ));
    Ok(total)
```

Also remove `use std::error::Error;` from the top of `github.rs` (it's needed until Task 4 — leave it for now) and remove the `tokio::sync::mpsc` inline type references from the `ingest_github` signature; replace with `mpsc::Sender` now that we have the import.

- [ ] **Step 5: Verify compile + line count**

```bash
cargo check --bin axon 2>&1 | grep "^error"
# Count lines in ingest_github function manually or with:
awk '/^pub async fn ingest_github/,/^}/' crates/ingest/github.rs | wc -l
```
Expected: no errors; function under 75 lines

- [ ] **Step 6: Run tests**

```bash
cargo test -p axon_cli ingest 2>&1 | tail -5
```
Expected: all pass

- [ ] **Step 7: Commit**

```bash
git add crates/ingest/github.rs
git commit -m "refactor(ingest): extract tally_results + fix progress sender in ingest_github (monolith fix)"
```

---

## Chunk 2: Cleanup

### Task 4: Migrate `Box<dyn Error>` → `anyhow::Result` across all four files

**Files:**
- Modify: `crates/ingest/github.rs`
- Modify: `crates/ingest/github/files.rs`
- Modify: `crates/ingest/github/wiki.rs`
- Modify: `crates/ingest/github/issues.rs`

`anyhow = "1"` is already in `Cargo.toml`. The `?` operator works unchanged because `anyhow::Error` absorbs any `E: Error + Send + Sync + 'static`. This is a pure type-signature change — no logic changes.

**Pattern to apply in each file:**

| Before | After |
|--------|-------|
| `use std::error::Error;` | `use anyhow::{Result, bail};` |
| `Result<T, Box<dyn Error>>` | `Result<T>` (anyhow's `Result`) |
| `Err(format!("...{e}").into())` | `bail!("...{e}")` |
| `.map_err(\|e\| format!("...{e}").into())` | `.map_err(\|e\| anyhow::anyhow!("...{e}"))` |

**Exception:** `ingest_github` in `github.rs` is the public entry point called by the CLI command. Its return type stays `Result<usize, Box<dyn Error>>` to avoid requiring the CLI command to take anyhow. All internal helpers change to `anyhow::Result`.

Actually — `anyhow::Error` implements `Into<Box<dyn Error>>` so the `?` in the CLI command still works even if `ingest_github` returns `anyhow::Result`. Change `ingest_github` to `anyhow::Result` too for consistency. The CLI command does `ingest_github(...).await?` where the outer function returns `Result<_, Box<dyn Error>>` — this works via `From<anyhow::Error>`.

- [ ] **Step 1: Migrate `github/wiki.rs`**

Change:
```rust
use std::error::Error;
```
To:
```rust
use anyhow::{Result, bail};
```

Change all function signatures from `Result<T, Box<dyn Error>>` to `Result<T>`.

Change error constructions:
```rust
// BEFORE:
return Err(format!("wiki clone failed: {}", stderr.trim()).into());
// ...
.map_err(|e| format!("git not found or failed to start: {e}"))?;

// AFTER:
bail!("wiki clone failed: {}", stderr.trim());
// ...
.map_err(|e| anyhow::anyhow!("git not found or failed to start: {e}"))?;
```

- [ ] **Step 2: Migrate `github/files.rs`**

Same pattern. Change:
```rust
use std::error::Error;
```
To:
```rust
use anyhow::{Result, bail};
```

Change all signatures. Change error constructions:
```rust
// BEFORE:
return Err(format!("git clone failed (exit {}): {}", output.status, stderr.trim()).into());
.map_err(|e| format!("git not found or failed to start: {e}"))?
.map_err(|e| format!("failed to recreate tmp dir for unauthenticated retry: {e}"))?

// AFTER:
bail!("git clone failed (exit {}): {}", output.status, stderr.trim());
.map_err(|e| anyhow::anyhow!("git not found or failed to start: {e}"))?
.map_err(|e| anyhow::anyhow!("failed to recreate tmp dir for unauthenticated retry: {e}"))?
```

- [ ] **Step 3: Migrate `github/issues.rs`**

Same pattern. Change `use std::error::Error;` to `use anyhow::Result;`. Change signatures. No custom error constructions in this file — only `?` propagation, which is unchanged.

- [ ] **Step 4: Migrate `github.rs`**

Change `use std::error::Error;` to `use anyhow::{Result, bail};`.

Change all internal helper signatures. Change error constructions:
```rust
// BEFORE (build_octocrab):
fn build_octocrab(cfg: &Config) -> Result<Octocrab, Box<dyn Error>> {

// AFTER:
fn build_octocrab(cfg: &Config) -> Result<Octocrab> {
```

```rust
// BEFORE (ingest_github):
.ok_or_else(|| format!("invalid GitHub repo: {repo}"))?;

// AFTER:
.ok_or_else(|| anyhow::anyhow!("invalid GitHub repo: {repo}"))?;
```

Update `tally_results` signature:
```rust
// BEFORE:
fn tally_results(results: [(&str, Result<usize, Box<dyn Error>>); 5], repo: &str) -> (usize, usize, usize)

// AFTER:
fn tally_results(results: [(&str, Result<usize>); 5], repo: &str) -> (usize, usize, usize)
```

- [ ] **Step 5: Verify compile**

```bash
cargo check --bin axon 2>&1 | grep "^error"
```
Expected: no output

- [ ] **Step 6: Run all ingest tests**

```bash
cargo test -p axon_cli ingest -- --nocapture 2>&1 | tail -10
```
Expected: all pass

- [ ] **Step 7: Commit**

```bash
git add crates/ingest/github.rs crates/ingest/github/files.rs crates/ingest/github/wiki.rs crates/ingest/github/issues.rs
git commit -m "refactor(ingest): migrate github ingest module from Box<dyn Error> to anyhow::Result"
```

---

### Task 5: Replace `.ok()` URL domain extraction with `"github.com"` constant

**Files:**
- Modify: `crates/ingest/github.rs` (line ~161)
- Modify: `crates/ingest/github/wiki.rs` (in `build_wiki_docs`)
- Modify: `crates/ingest/github/issues.rs` (lines ~73, ~156)

Every URL in this module is constructed as `format!("https://github.com/...")`. Parsing it to extract the host is pointless — the host is always `github.com`. The parse-and-extract pattern also brings in a `spider::url::Url` dependency that can be removed from two of these files.

- [ ] **Step 1: Replace in `github.rs` (`embed_repo_metadata`)**

Replace:
```rust
    let domain = spider::url::Url::parse(&url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "github.com".to_string());
```
With:
```rust
    let domain = "github.com".to_string();
```

- [ ] **Step 2: Replace in `wiki.rs` (`build_wiki_docs`)**

The new `build_wiki_docs` function (from Task 2) already uses `"github.com".to_string()` directly — verify this is already in place. No change needed if Task 2 was done correctly.

- [ ] **Step 3: Replace in `issues.rs` (both functions)**

In `ingest_issues` (~line 73) and `ingest_pull_requests` (~line 156), replace both occurrences of:
```rust
                let domain = spider::url::Url::parse(&url)
                    .ok()
                    .and_then(|u| u.host_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| "github.com".to_string());
```
With:
```rust
                let domain = "github.com".to_string();
```

- [ ] **Step 4: Remove unused spider imports**

Check each file for `spider::` usage after the removal. In `issues.rs`, `spider` is no longer used — remove any implicit `spider::` path references. In `github.rs`, verify `spider` is not used elsewhere in the file before removing.

```bash
grep "spider::" crates/ingest/github.rs crates/ingest/github/issues.rs crates/ingest/github/wiki.rs
```
Expected: no matches (or only in places unrelated to URL parsing)

- [ ] **Step 5: Verify compile**

```bash
cargo check --bin axon 2>&1 | grep "^error"
```
Expected: no output

- [ ] **Step 6: Run tests**

```bash
cargo test -p axon_cli ingest 2>&1 | tail -5
```
Expected: all pass

- [ ] **Step 7: Commit**

```bash
git add crates/ingest/github.rs crates/ingest/github/issues.rs crates/ingest/github/wiki.rs
git commit -m "refactor(ingest): replace spider URL parse with github.com constant in domain extraction"
```

---

### Task 6: Replace `walk_dir_recursive` (Box::pin) with iterative stack in `wiki.rs`

**Files:**
- Modify: `crates/ingest/github/wiki.rs`

`walk_dir_recursive` uses `Box::pin` for recursion, allocating on the heap for every directory level. The iterative stack pattern in `files.rs` (`collect_indexable_files`) is superior. Replace the recursive function with an iterative equivalent local to `wiki.rs`.

- [ ] **Step 1: Replace `walk_dir_recursive` with iterative version**

Replace the entire `walk_dir_recursive` function (lines 11–26) with:
```rust
/// Walk a directory iteratively and collect all file paths.
/// Skips `.git` directories. Uses an explicit stack to avoid recursive heap allocation.
async fn collect_wiki_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.file_name().and_then(|n| n.to_str()) == Some(".git") {
                continue;
            }
            if entry.file_type().await?.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }

    Ok(files)
}
```

- [ ] **Step 2: Update `build_wiki_docs` to call the new function**

In `build_wiki_docs`, change:
```rust
    let all_files = walk_dir_recursive(Path::new(tmp_path)).await?;
```
To:
```rust
    let all_files = collect_wiki_files(Path::new(tmp_path)).await?;
```

- [ ] **Step 3: Verify compile**

```bash
cargo check --bin axon 2>&1 | grep "^error"
```
Expected: no output

- [ ] **Step 4: Run tests**

```bash
cargo test -p axon_cli ingest 2>&1 | tail -5
```
Expected: all pass

- [ ] **Step 5: Commit**

```bash
git add crates/ingest/github/wiki.rs
git commit -m "refactor(ingest): replace recursive Box::pin walk with iterative stack in wiki.rs"
```

---

### Task 7: `Arc<FileEmbedCtx>` — eliminate per-file Config clone

**Files:**
- Modify: `crates/ingest/github/files.rs`

`FileEmbedCtx` contains a full `Config` clone and is cloned once per file in the concurrent stream. For a 500-file repo that's 500 `Config` clones. Wrap in `Arc` so each "clone" is just a pointer increment.

- [ ] **Step 1: Add `Arc` import**

Add to the top of `files.rs`:
```rust
use std::sync::Arc;
```

- [ ] **Step 2: Remove `#[derive(Clone)]` from `FileEmbedCtx`**

Change:
```rust
#[derive(Clone)]
struct FileEmbedCtx {
```
To:
```rust
struct FileEmbedCtx {
```

- [ ] **Step 3: Update `embed_files` to wrap in `Arc`**

Change:
```rust
    let ctx = FileEmbedCtx {
        cfg: cfg.clone(),
        // ...
    };
    let mut failed = 0usize;
    let docs = collect_embed_docs(&ctx, file_items, files_total, progress_tx, &mut failed).await;
```
To:
```rust
    let ctx = Arc::new(FileEmbedCtx {
        cfg: cfg.clone(),
        // ...
    });
    let mut failed = 0usize;
    let docs = collect_embed_docs(&ctx, file_items, files_total, progress_tx, &mut failed).await;
```

- [ ] **Step 4: Update `collect_embed_docs` signature and clone**

Change the signature from `ctx: &FileEmbedCtx` to `ctx: &Arc<FileEmbedCtx>` and the interior clone:
```rust
async fn collect_embed_docs(
    ctx: &Arc<FileEmbedCtx>,   // was: ctx: &FileEmbedCtx
    file_items: Vec<String>,
    files_total: usize,
    progress_tx: Option<&mpsc::Sender<serde_json::Value>>,
    failed: &mut usize,
) -> Vec<PreparedDoc> {
    // ...
    .map(|path| {
        let ctx = Arc::clone(ctx);   // was: let ctx = ctx.clone();
        async move { read_file_embed_doc(&ctx, &path).await }   // &*ctx for deref
    })
```

Note: `read_file_embed_doc` takes `&FileEmbedCtx`. `Arc<FileEmbedCtx>` derefs to `FileEmbedCtx` via `Deref`, so `&*ctx` or `ctx.as_ref()` gives `&FileEmbedCtx`. Update the call:
```rust
async move { read_file_embed_doc(&ctx, &path).await }
```
Since `ctx: Arc<FileEmbedCtx>` inside the `async move` closure, write it as:
```rust
async move { read_file_embed_doc(ctx.as_ref(), &path).await }
```

- [ ] **Step 5: Verify compile**

```bash
cargo check --bin axon 2>&1 | grep "^error"
```
Expected: no output

- [ ] **Step 6: Run tests**

```bash
cargo test -p axon_cli ingest 2>&1 | tail -5
```
Expected: all pass

- [ ] **Step 7: Final monolith check**

```bash
just precommit 2>&1 | tail -20
```
Expected: passes all checks

- [ ] **Step 8: Commit**

```bash
git add crates/ingest/github/files.rs
git commit -m "perf(ingest): Arc<FileEmbedCtx> — eliminate per-file Config clone in embed stream"
```

---

## Verification

After all tasks:

```bash
# Full gate
just verify

# Ingest-specific tests
cargo test -p axon_cli ingest -- --nocapture

# Monolith check
./scripts/enforce_monoliths.py 2>&1 | grep -E "FAIL|ERROR" || echo "clean"

# Live smoke test
./scripts/axon ingest steipete/mcporter --wait true 2>&1 | grep -E "(WARN|chunks embedded)"
```

Expected:
- `just verify` passes
- No monolith violations
- Smoke test: no WARN lines, `✓ N chunks embedded`
