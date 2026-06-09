# Unify Code/File Ingestion Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract one shared filesystem→chunked-`PreparedDoc` engine and route every git provider (GitHub, GitLab, generic Git, Gitea) and the local `embed <dir>` path through it, so code-aware (tree-sitter) chunking, symbol metadata, and the canonical `code_*`/`git_*`/`symbol_*` payload are produced uniformly instead of via four divergent copies.

**Architecture:** PR #187 already built the symbol-aware chunker (`chunk_code_chunks` → `Vec<CodeChunk>`), the canonical `git_*`/`code_*`/`symbol_*` payload (`build_git_payload`), and the v6 schema — but only GitHub uses the full chunker, and the walker + chunk loop are copy-pasted across providers. This plan introduces `src/vector/ops/file_ingest.rs` (a shared walker + `chunk_file` adapter) and rewires all callers onto it, fixing the generic-Git/Gitea prose-chunking bug and the GitLab missing-symbols gap (bead `axon_rust-wavn`) in one place.

**Tech Stack:** Rust 2024, tokio, tree-sitter (`text_splitter::CodeSplitter`), serde_json, Qdrant payloads.

---

## Preconditions (do these before Task 1)

This plan targets the **`codex/axon_rust-xkv0` branch (PR #187)**. It depends on the local-embed file selection predicates from **PR #188** (`src/vector/ops/input/select.rs`: `is_pruned_dir`, `is_binary_ext`, `should_chunk_as_code`).

- [ ] **P0: Land #188 and rebase #187 onto it**

```bash
# Merge #188 first (small, already green), then:
cd /home/jmagar/workspace/axon/.worktrees/codex/axon_rust-xkv0
git fetch origin
git rebase origin/main
# Confirm select.rs is present:
test -f src/vector/ops/input/select.rs && echo "select.rs present" || echo "MISSING — do not proceed"
```

Expected: `select.rs present`. If missing, stop — Tasks 1 and 7 import it.

- [ ] **P1: Confirm the symlink to the web build dir exists so the crate compiles in this worktree**

```bash
ls apps/web/out >/dev/null 2>&1 || ln -s /home/jmagar/workspace/axon/apps/web/out apps/web/out
cargo check --bin axon
```

Expected: `Finished`. (`apps/web/out` is a gitignored build artifact; the symlink is worktree-local and never committed.)

---

## File Structure

| File | Responsibility |
|------|----------------|
| `src/vector/ops/file_ingest.rs` | **New.** Shared engine: `SelectionPolicy`, `collect_files()` (recursive, resilient, symlink-skip), `chunk_file()` (`Vec<CodeChunk>` adapter), `chunking_method()`, plus the `text_chunks`/`next_search_start`/`line_for_byte` helpers moved out of GitHub's `prepare.rs`. |
| `src/vector/ops/file_ingest_tests.rs` | **New.** Sidecar tests for the engine. |
| `src/vector/ops.rs` | Add `pub mod file_ingest;`. |
| `src/ingest/github/files/prepare.rs` | Rewire onto the shared engine; delete the now-moved helpers. |
| `src/ingest/gitlab/files.rs` | Rewire onto shared engine; emit per-chunk symbols + `code_*` line metadata (closes `axon_rust-wavn`). |
| `src/ingest/gitlab/embed.rs` | Add `gitlab_file_chunk_payload()` for per-chunk file payloads. |
| `src/ingest/generic_git.rs` | Rewire `file_doc` → `file_docs` (code-aware, per-chunk, symbols). Fixes Gitea too. |
| `src/vector/ops/tei/prepare.rs` | Rewire local-embed dir reader onto the shared engine; emit `code_*`/`symbol_*` for local code files; keep crawl-manifest handling. |
| `src/ingest/CLAUDE.md`, `src/vector/CLAUDE.md`, `CHANGELOG.md`, `Cargo.toml` | Docs + version bump. |

**No `PAYLOAD_SCHEMA_VERSION` bump:** all target fields (`symbol_*`, `code_line_*`, `code_chunking_method`) already exist in `build_git_payload` and are indexed (v6 from #187). This plan only *populates* them for more providers — it does not add fields.

---

## Phase 1 — Shared engine + GitHub rewire (no behavior change)

### Task 1: Create the shared `file_ingest` engine

**Files:**
- Create: `src/vector/ops/file_ingest.rs`
- Create: `src/vector/ops/file_ingest_tests.rs`
- Modify: `src/vector/ops.rs` (add module declaration)

- [ ] **Step 1: Declare the module**

In `src/vector/ops.rs`, add alongside the other `pub mod` lines:

```rust
pub mod file_ingest;
```

Run: `grep -n "pub mod file_ingest;" src/vector/ops.rs`
Expected: one match.

- [ ] **Step 2: Write the engine source**

Create `src/vector/ops/file_ingest.rs` with the walker + chunk adapter. The chunk logic is lifted verbatim from `src/ingest/github/files/prepare.rs` (`code_or_text_chunks`, `text_chunks`, `next_search_start`, `line_for_byte`, `chunking_method`) so GitHub's behavior is preserved exactly.

```rust
//! Shared filesystem → chunked-document engine.
//!
//! One recursive walker + one chunk-selection adapter, used by every git
//! provider (after clone) and the local `embed <dir>` path. Providers supply
//! only the per-file URL and payload; this module owns file selection and the
//! code-vs-prose chunk decision so all callers produce identical `CodeChunk`
//! shapes and symbol metadata.

use std::path::{Path, PathBuf};

use crate::core::logging::log_warn;
use crate::ingest::github::{is_indexable_doc_path, is_indexable_source_path};
use crate::vector::ops::input::select;
use crate::vector::ops::input::{
    CHUNK_OVERLAP, chunk_text,
    code::{CodeChunk, chunk_code_chunks, supports_tree_sitter_chunking},
};

/// Which files a directory walk should yield.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionPolicy {
    /// Git-repo ingest: curated allowlist of doc/source extensions.
    Allowlist { include_source: bool },
    /// Local `embed <dir>`: permissive — everything except binary extensions.
    Permissive,
}

/// Recursively collect files under `root` per `policy`.
///
/// Resilience: the top-level `root` read is a hard error (nothing to embed if
/// the target is unreadable), but an unreadable subdirectory is logged and
/// skipped. Pruned directories (`select::is_pruned_dir`: `.git`, `node_modules`,
/// `target`, …) are never descended into. Symlinks are skipped (their
/// `file_type` is neither file nor dir). Returned paths are sorted.
pub async fn collect_files(
    root: &Path,
    policy: SelectionPolicy,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut at_root = true;
    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(e) if at_root => {
                return Err(format!("invalid ingest directory {}: {e}", dir.display()).into());
            }
            Err(e) => {
                log_warn(&format!(
                    "command=ingest skip_unreadable_dir path={} err={e}",
                    dir.display()
                ));
                at_root = false;
                continue;
            }
        };
        at_root = false;
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(e) => {
                    log_warn(&format!(
                        "command=ingest dir_iter_error path={} err={e}",
                        dir.display()
                    ));
                    break;
                }
            };
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let Ok(file_type) = entry.file_type().await else {
                log_warn(&format!(
                    "command=ingest skip_unknown_type path={}",
                    path.display()
                ));
                continue;
            };
            if file_type.is_dir() {
                if !select::is_pruned_dir(name) {
                    stack.push(path);
                }
            } else if file_type.is_file() && include_file(&path, root, policy) {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn include_file(path: &Path, root: &Path, policy: SelectionPolicy) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    match policy {
        SelectionPolicy::Permissive => {
            !select::is_binary_ext(crate::vector::ops::input::classify::path_extension(name))
        }
        SelectionPolicy::Allowlist { include_source } => {
            let Ok(rel) = path.strip_prefix(root) else {
                return false;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            is_indexable_doc_path(&rel) || (include_source && is_indexable_source_path(&rel))
        }
    }
}

/// Chunk one file's content into `CodeChunk`s: AST-aware via tree-sitter when a
/// grammar exists for `ext`, otherwise prose chunks adapted to `CodeChunk`.
/// CPU-bound — callers embedding many files should wrap in `spawn_blocking`.
pub fn chunk_file(content: &str, ext: &str) -> Vec<CodeChunk> {
    chunk_code_chunks(content, ext).unwrap_or_else(|| text_chunks(content))
}

/// Report the chunking method for one chunk: tree-sitter when the grammar is
/// supported (or a symbol was found), else prose.
pub fn chunking_method(ext: &str, chunk: &CodeChunk) -> &'static str {
    if chunk.symbol_kind.is_some() || supports_tree_sitter_chunking(ext) {
        "tree_sitter"
    } else {
        "prose"
    }
}

fn text_chunks(text: &str) -> Vec<CodeChunk> {
    chunk_text(text)
        .into_iter()
        .scan(0usize, |search_start, chunk| {
            let byte_offset = text[*search_start..]
                .find(chunk.as_str())
                .map(|pos| *search_start + pos)
                .unwrap_or(*search_start);
            let chunk_len = chunk.len();
            *search_start = next_search_start(text, byte_offset, chunk_len);
            let line_start = line_for_byte(text, byte_offset);
            let line_end = line_for_byte(text, byte_offset + chunk_len);
            Some(CodeChunk {
                text: chunk,
                byte_start: byte_offset,
                byte_end: byte_offset + chunk_len,
                start_line: line_start,
                end_line: line_end,
                declaration_start_line: line_start,
                declaration_end_line: line_end,
                symbol_name: None,
                symbol_kind: None,
            })
        })
        .collect()
}

fn next_search_start(text: &str, byte_offset: usize, chunk_len: usize) -> usize {
    let chunk_end = (byte_offset + chunk_len).min(text.len());
    let mut pos = chunk_end;
    for _ in 0..CHUNK_OVERLAP {
        if pos == 0 {
            break;
        }
        pos -= 1;
        while pos > 0 && !text.is_char_boundary(pos) {
            pos -= 1;
        }
    }
    pos
}

fn line_for_byte(content: &str, byte: usize) -> u32 {
    let capped = byte.min(content.len());
    content[..capped].bytes().filter(|b| *b == b'\n').count() as u32 + 1
}

#[cfg(test)]
#[path = "file_ingest_tests.rs"]
mod tests;
```

- [ ] **Step 3: Write the engine tests**

Create `src/vector/ops/file_ingest_tests.rs`:

```rust
use super::*;
use tempfile::TempDir;

#[tokio::test]
async fn permissive_recurses_prunes_and_skips_binary() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    tokio::fs::create_dir_all(root.join("a/b")).await.unwrap();
    tokio::fs::write(root.join("a/b/c.rs"), "fn x() {}").await.unwrap();
    tokio::fs::write(root.join("r.md"), "# hi").await.unwrap();
    tokio::fs::write(root.join("img.png"), "x").await.unwrap();
    tokio::fs::create_dir_all(root.join("node_modules")).await.unwrap();
    tokio::fs::write(root.join("node_modules/x.js"), "1").await.unwrap();

    let files = collect_files(root, SelectionPolicy::Permissive).await.unwrap();
    let names: Vec<String> = files.iter().map(|p| p.to_string_lossy().to_string()).collect();
    assert_eq!(files.len(), 2, "{names:?}");
    assert!(names.iter().any(|n| n.ends_with("a/b/c.rs")));
    assert!(names.iter().any(|n| n.ends_with("r.md")));
    assert!(!names.iter().any(|n| n.ends_with("img.png")));
    assert!(!names.iter().any(|n| n.contains("node_modules")));
}

#[tokio::test]
async fn allowlist_excludes_non_source_when_include_source_false() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    tokio::fs::write(root.join("a.rs"), "fn x() {}").await.unwrap();
    tokio::fs::write(root.join("README.md"), "# hi").await.unwrap();

    let docs_only = collect_files(root, SelectionPolicy::Allowlist { include_source: false })
        .await
        .unwrap();
    assert_eq!(docs_only.len(), 1);
    assert!(docs_only[0].to_string_lossy().ends_with("README.md"));

    let with_src = collect_files(root, SelectionPolicy::Allowlist { include_source: true })
        .await
        .unwrap();
    assert_eq!(with_src.len(), 2);
}

#[test]
fn chunk_file_uses_ast_for_rust_and_sets_symbol() {
    let src = "fn alpha() {\n    let _ = 1;\n}\n\nfn beta() {\n    let _ = 2;\n}\n";
    let chunks = chunk_file(src, "rs");
    assert!(!chunks.is_empty());
    assert!(
        chunks.iter().any(|c| c.symbol_kind.is_some()),
        "expected at least one symbol-bearing chunk"
    );
    assert_eq!(chunking_method("rs", &chunks[0]), "tree_sitter");
}

#[test]
fn chunk_file_falls_back_to_prose_for_unknown_ext() {
    let text = "plain prose ".repeat(400);
    let chunks = chunk_file(&text, "md");
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|c| c.symbol_kind.is_none()));
    assert_eq!(chunking_method("md", &chunks[0]), "prose");
}
```

- [ ] **Step 4: Run the engine tests**

Run: `cargo test --lib -- file_ingest`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add src/vector/ops.rs src/vector/ops/file_ingest.rs src/vector/ops/file_ingest_tests.rs
git commit -m "feat(ingest): shared file_ingest engine (walker + chunk_file)"
```

### Task 2: Rewire GitHub onto the engine (behavior-preserving)

**Files:**
- Modify: `src/ingest/github/files/prepare.rs`
- Test: `src/ingest/github/files/prepare_tests.rs` (existing golden tests must stay green)

- [ ] **Step 1: Replace the local walker + chunk helpers with engine calls**

In `src/ingest/github/files/prepare.rs`:

Delete these now-shared items (moved to `file_ingest`): `next_search_start`, `code_or_text_chunks`, `text_chunks`, `chunking_method`, `line_for_byte`. Replace the `collect_indexable_files` body and the chunk call.

Change the imports block to:

```rust
use crate::vector::ops::file_ingest::{SelectionPolicy, chunk_file, chunking_method, collect_files};
use crate::vector::ops::input::classify::{classify_file_type, is_test_path, language_name, path_extension};
use crate::vector::ops::input::code::{CodeChunk, code_symbol_extraction_status};
use crate::vector::ops::PreparedDoc;
```

Replace `collect_indexable_files` with a thin wrapper that returns repo-relative strings (callers downstream still expect `Vec<String>` relative paths):

```rust
/// Collect repo-relative indexable file paths under `root`.
pub(super) async fn collect_indexable_files(
    root: &Path,
    include_source: bool,
) -> Result<Vec<String>> {
    let abs = collect_files(root, SelectionPolicy::Allowlist { include_source })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(abs
        .into_iter()
        .filter_map(|p| p.strip_prefix(root).ok().map(|r| r.to_string_lossy().to_string()))
        .collect())
}
```

In `read_file_embed_docs`, replace the `spawn_blocking` chunk block body:

```rust
    let ext = file_extension(path);
    let ext_for_chunk = ext.clone();
    let (chunks, text) = tokio::task::spawn_blocking(move || {
        let chunks = chunk_file(&text, &ext_for_chunk);
        (chunks, text)
    })
    .await
    .map_err(|e| format!("chunk_file panicked: {e}"))?;
    if chunks.is_empty() {
        return Ok(Vec::new());
    }
```

(`chunking_method` and `code_symbol_extraction_status` are now imported; `prepared_doc_for_chunk` keeps calling them unchanged.)

- [ ] **Step 2: Verify the existing GitHub golden tests still pass**

Run: `cargo test --lib -- ingest::github::files`
Expected: PASS (the v6 payload + symbol assertions in `prepare_tests.rs`/`meta_tests.rs` are unchanged because output is identical).

- [ ] **Step 3: Verify full lib still compiles + passes**

Run: `cargo test --lib`
Expected: all passed (2487+ baseline).

- [ ] **Step 4: Commit**

```bash
git add src/ingest/github/files/prepare.rs
git commit -m "refactor(ingest): GitHub file ingest uses shared file_ingest engine"
```

---

## Phase 2 — GitLab: code-aware chunking + symbols (closes `axon_rust-wavn`)

### Task 3: Add a per-chunk GitLab file payload builder

**Files:**
- Modify: `src/ingest/gitlab/embed.rs`
- Test: `src/ingest/gitlab/embed_tests.rs` (create if absent; else append + wire `#[path]`)

- [ ] **Step 1: Write a failing test for the per-chunk payload**

Append to `src/ingest/gitlab/embed_tests.rs` (if the sidecar doesn't exist, create it and add `#[cfg(test)] #[path = "embed_tests.rs"] mod tests;` to `embed.rs`):

```rust
#[test]
fn gitlab_file_chunk_payload_sets_code_and_symbol_fields() {
    use crate::vector::ops::input::code::{CodeChunk, SymbolKind};
    let target = test_target();      // existing helper or inline a GitLabTarget
    let project = test_project();    // existing helper or inline a GitLabProject
    let chunk = CodeChunk {
        text: "fn x() {}".into(),
        byte_start: 0,
        byte_end: 9,
        start_line: 10,
        end_line: 12,
        declaration_start_line: 10,
        declaration_end_line: 12,
        symbol_name: Some("x".into()),
        symbol_kind: Some(SymbolKind::Function),
    };
    let payload = gitlab_file_chunk_payload(
        &target, &project, "src/lib.rs", "main", &chunk, "tree_sitter", "ok",
    );
    assert_eq!(payload["git_content_kind"], "file");
    assert_eq!(payload["code_file_path"], "src/lib.rs");
    assert_eq!(payload["code_line_start"], 10);
    assert_eq!(payload["code_line_end"], 12);
    assert_eq!(payload["code_chunking_method"], "tree_sitter");
    assert_eq!(payload["symbol_name"], "x");
    assert_eq!(payload["symbol_kind"], "function");
    assert_eq!(payload["symbol_extraction_status"], "ok");
}
```

(If `test_target`/`test_project` helpers don't exist, construct minimal `GitLabTarget`/`GitLabProject` literals inline — check `src/ingest/gitlab/types.rs` for required fields.)

- [ ] **Step 2: Run it — expect failure**

Run: `cargo test --lib -- gitlab_file_chunk_payload`
Expected: FAIL ("cannot find function `gitlab_file_chunk_payload`").

- [ ] **Step 3: Implement `gitlab_file_chunk_payload`**

Add to `src/ingest/gitlab/embed.rs` (reuses the existing `build_git_payload`, `language_name`, `classify_file_type`, `is_test_path`, `path_extension` imports already present in this file):

```rust
/// Build a canonical per-chunk GitLab file payload with code + symbol metadata.
pub(crate) fn gitlab_file_chunk_payload(
    target: &GitLabTarget,
    project: &GitLabProject,
    rel: &str,
    branch: &str,
    chunk: &crate::vector::ops::input::code::CodeChunk,
    chunking_method: &str,
    symbol_status: &str,
) -> serde_json::Value {
    let owner = {
        let path = &target.namespace_path;
        path.rfind('/').map(|i| path[..i].to_string())
    };
    build_git_payload(&GitPayload {
        provider: "gitlab".to_string(),
        host: target.host.clone(),
        owner,
        repo: target.project.clone(),
        content_kind: "file",
        branch: Some(branch.to_string()),
        file_path: Some(rel.to_string()),
        file_language: Some(language_name(path_extension(rel)).to_string()),
        file_type: Some(classify_file_type(rel).to_string()),
        file_is_test: Some(is_test_path(rel)),
        line_start: Some(chunk.start_line),
        line_end: Some(chunk.end_line),
        chunking_method: Some(chunking_method.to_string()),
        symbol_name: chunk.symbol_name.clone(),
        symbol_kind: chunk.symbol_kind_str().map(str::to_string),
        symbol_extraction_status: Some(symbol_status.to_string()),
        meta: Some(serde_json::json!({
            "namespace_path": target.namespace_path,
            "visibility": project.visibility,
            "last_activity_at": project.last_activity_at,
            "default_branch": project.default_branch,
        })),
        ..GitPayload::default()
    })
}
```

- [ ] **Step 4: Run the test — expect pass**

Run: `cargo test --lib -- gitlab_file_chunk_payload`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ingest/gitlab/embed.rs src/ingest/gitlab/embed_tests.rs
git commit -m "feat(ingest): canonical per-chunk GitLab file payload with symbols"
```

### Task 4: Route GitLab `embed_files` through the engine

**Files:**
- Modify: `src/ingest/gitlab/files.rs`

- [ ] **Step 1: Replace the walker + chunk loop**

In `src/ingest/gitlab/files.rs`: delete the local `collect_files` function. Update imports:

```rust
use crate::vector::ops::file_ingest::{SelectionPolicy, chunk_file, chunking_method, collect_files};
use crate::vector::ops::input::code::code_symbol_extraction_status;
use crate::vector::ops::PreparedDoc;
use super::embed::{embed_docs, gitlab_file_chunk_payload};
```

Replace the body of `embed_files`'s file loop (the block from `let files = collect_files(...)` through the doc push) with:

```rust
    let files = collect_files(tmp.path(), SelectionPolicy::Allowlist { include_source })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let total = files.len();
    let mut docs = Vec::new();
    for (index, file) in files.into_iter().enumerate() {
        let rel = file
            .strip_prefix(tmp.path())?
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(content) = tokio::fs::read_to_string(&file).await else {
            continue;
        };
        let ext = std::path::Path::new(&rel)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let chunks = match tokio::task::spawn_blocking(move || {
            let chunks = chunk_file(&content, &ext);
            (chunks, content, ext)
        })
        .await
        {
            Ok((chunks, content, ext)) => {
                let status = code_symbol_extraction_status(&content, &ext, &chunks);
                (chunks, ext, status)
            }
            Err(e) => {
                tracing::warn!(path = %rel, error = %e, "spawn_blocking panicked; skipping file");
                continue;
            }
        };
        let (chunks, ext, symbol_status) = chunks;
        for chunk in chunks {
            let method = chunking_method(&ext, &chunk);
            docs.push(PreparedDoc {
                url: format!("{}/-/blob/{}/{}#L{}-L{}", target.web_url, branch, rel, chunk.start_line, chunk.end_line),
                domain: target.host.clone(),
                chunks: vec![chunk.text.clone()],
                source_type: "gitlab".to_string(),
                content_type: "text",
                title: Some(rel.clone()),
                extra: Some(gitlab_file_chunk_payload(
                    target, project, &rel, branch, &chunk, method, symbol_status,
                )),
                extractor_name: None,
                structured: None,
            });
        }
        if (index + 1) % 25 == 0 || index + 1 == total {
            reporter
                .report(serde_json::json!({"files_done": index + 1, "files_total": total}))
                .await;
        }
    }
```

Note: `spawn_blocking` now moves `content` and `ext` in and returns them so `code_symbol_extraction_status` can run after (it needs the original content). `chunk.text` is cloned because the URL also reads `chunk.start_line`/`end_line` (Copy fields) before the move.

- [ ] **Step 2: Verify compile**

Run: `cargo check --bin axon`
Expected: `Finished`.

- [ ] **Step 3: Run gitlab unit tests**

Run: `cargo test --lib -- gitlab`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/ingest/gitlab/files.rs
git commit -m "feat(ingest): GitLab files use shared engine + per-chunk symbols (axon_rust-wavn)"
```

---

## Phase 3 — generic Git + Gitea: code-aware chunking + symbols

### Task 5: Convert generic_git `file_doc` → per-chunk `file_docs`

**Files:**
- Modify: `src/ingest/generic_git.rs`
- Test: `src/ingest/generic_git_tests.rs` (append; wire `#[path]` if needed)

- [ ] **Step 1: Replace `file_doc` with a code-aware `file_docs`**

In `src/ingest/generic_git.rs`, update imports:

```rust
use crate::vector::ops::file_ingest::{chunk_file, chunking_method};
use crate::vector::ops::input::classify::{classify_file_type, is_test_path, language_name, path_extension};
use crate::vector::ops::input::code::code_symbol_extraction_status;
```

Replace `file_doc` (returns `Option<PreparedDoc>`) with `file_docs` (returns `Vec<PreparedDoc>`):

```rust
async fn file_docs(
    root: &Path,
    target: &GenericGitTarget,
    branch: &str,
    file: PathBuf,
    source_type: &str,
    provider: &str,
) -> Result<Vec<PreparedDoc>> {
    let rel = file
        .strip_prefix(root)?
        .to_string_lossy()
        .replace('\\', "/");
    let Ok(content) = tokio::fs::read_to_string(&file).await else {
        return Ok(Vec::new());
    };
    let ext = path_extension(&rel).to_ascii_lowercase();
    let (chunks, content) = tokio::task::spawn_blocking(move || {
        let chunks = chunk_file(&content, &ext);
        (chunks, content)
    })
    .await
    .map_err(|e| anyhow!("chunk_file panicked: {e}"))?;
    if chunks.is_empty() {
        return Ok(Vec::new());
    }
    let ext = path_extension(&rel).to_ascii_lowercase();
    let symbol_status = code_symbol_extraction_status(&content, &ext, &chunks);
    let lang = language_name(&ext).to_string();
    let ftype = classify_file_type(&rel).to_string();
    let is_test = is_test_path(&rel);
    let mut docs = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let method = chunking_method(&ext, &chunk);
        let extra = build_git_payload(&GitPayload {
            provider: provider.to_string(),
            host: target.host.clone(),
            owner: None,
            repo: target.name.clone(),
            content_kind: "file",
            branch: Some(branch.to_string()),
            file_path: Some(rel.clone()),
            file_language: Some(lang.clone()),
            file_type: Some(ftype.clone()),
            file_is_test: Some(is_test),
            line_start: Some(chunk.start_line),
            line_end: Some(chunk.end_line),
            chunking_method: Some(method.to_string()),
            symbol_name: chunk.symbol_name.clone(),
            symbol_kind: chunk.symbol_kind_str().map(str::to_string),
            symbol_extraction_status: Some(symbol_status.to_string()),
            meta: Some(serde_json::json!({ "clone_url": target.clone_url })),
            ..GitPayload::default()
        });
        docs.push(PreparedDoc {
            url: format!("{}#{}:{}#L{}-L{}", target.web_url, branch, rel, chunk.start_line, chunk.end_line),
            domain: target.host.clone(),
            chunks: vec![chunk.text],
            source_type: source_type.to_string(),
            content_type: "text",
            title: Some(rel.clone()),
            extra: Some(extra),
            extractor_name: None,
            structured: None,
        });
    }
    Ok(docs)
}
```

- [ ] **Step 2: Update the caller in `ingest_git_repository`**

Replace the loop body that calls `file_doc`:

```rust
    for (index, file) in files.into_iter().enumerate() {
        let mut file_docs = file_docs(tmp.path(), &target, &branch, file, source_type, provider).await?;
        docs.append(&mut file_docs);
        if (index + 1) % 25 == 0 || index + 1 == total {
            reporter
                .report(serde_json::json!({"files_done": index + 1, "files_total": total}))
                .await;
        }
    }
```

- [ ] **Step 3: Write a failing test for code-aware generic chunking**

Append to `src/ingest/generic_git_tests.rs`:

```rust
#[tokio::test]
async fn generic_file_docs_chunk_rust_as_code_with_symbols() {
    use crate::ingest::generic_git::GenericGitTarget; // adjust to actual visibility
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("lib.rs"), "fn alpha() {}\n\nfn beta() {}\n").unwrap();
    let target = GenericGitTarget {
        host: "example.com".into(),
        name: "repo".into(),
        clone_url: "https://example.com/r.git".into(),
        web_url: "https://example.com/r".into(),
        // …fill remaining required fields from generic_git.rs
    };
    let docs = super::file_docs(root, &target, "main", root.join("lib.rs"), "git", "git")
        .await
        .unwrap();
    assert!(!docs.is_empty());
    let extra = docs[0].extra.as_ref().unwrap();
    assert_eq!(extra["code_chunking_method"], "tree_sitter");
    assert_eq!(extra["code_file_type"], "source");
    assert!(docs.iter().any(|d| d.extra.as_ref().unwrap()["symbol_kind"] == "function"));
}
```

(If `file_docs`/`GenericGitTarget` aren't reachable from the sidecar's module path, mark them `pub(crate)` and import via `super::`. Check existing `generic_git_tests.rs` for the established pattern.)

- [ ] **Step 4: Run it**

Run: `cargo test --lib -- generic_file_docs_chunk_rust`
Expected: PASS.

- [ ] **Step 5: Confirm Gitea inherits (no code change)**

Gitea routes file ingest through `ingest_git_repository` (`src/ingest/gitea.rs:7`). Verify it now compiles and its tests pass:

Run: `cargo test --lib -- gitea generic_git`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/ingest/generic_git.rs src/ingest/generic_git_tests.rs
git commit -m "feat(ingest): generic Git + Gitea code-aware chunking with symbols"
```

---

## Phase 4 — Local `embed <dir>` onto the engine

### Task 6: Route the local directory reader through the shared engine

**Files:**
- Modify: `src/vector/ops/tei/prepare.rs`
- Modify: `src/vector/ops/tei/prepare_tests.rs`

- [ ] **Step 1: Replace `collect_embed_files` with the shared walker**

In `src/vector/ops/tei/prepare.rs`: delete the local `collect_embed_files` function. Update the dir branch of `read_inputs` to call the engine, and replace the `select_chunks` helper to emit `CodeChunk`-derived metadata for local code files.

Update imports:

```rust
use crate::vector::ops::file_ingest::{SelectionPolicy, chunk_file, chunking_method, collect_files};
use crate::vector::ops::input::classify::{classify_file_type, is_test_path, language_name, path_extension};
use crate::vector::ops::input::code::code_symbol_extraction_status;
```

In `read_inputs`, change the directory branch's collection call:

```rust
        Ok(meta) if meta.is_dir() => {
            let manifest_urls = read_manifest_url_map(&path);
            let files = collect_files(&path, SelectionPolicy::Permissive)
                .await
                .map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;
            // …unchanged manifest lookup + skip-on-error read loop…
        }
```

- [ ] **Step 2: Emit code/symbol metadata for local code files**

Replace the call site in `prepare_embed_docs` that builds chunks. For local-path (non-http) code files, attach a canonical `code_*`/`symbol_*` payload; crawl/http docs keep `extra: None`. Replace the `select_chunks` call + `PreparedDoc` build with:

```rust
        let is_local_code = !url.starts_with("http") && select::should_chunk_as_code(&url);
        let (chunks_text, content_type, extra) = if is_local_code {
            let ext = path_extension(&url).to_ascii_lowercase();
            let raw_for_blocking = raw.clone();
            let ext_for_blocking = ext.clone();
            let code_chunks = tokio::task::spawn_blocking(move || {
                chunk_file(&raw_for_blocking, &ext_for_blocking)
            })
            .await
            .unwrap_or_default();
            if code_chunks.is_empty() {
                continue;
            }
            let status = code_symbol_extraction_status(&raw, &ext, &code_chunks);
            // One PreparedDoc per code chunk so symbol metadata is per-chunk.
            for chunk in code_chunks {
                let method = chunking_method(&ext, &chunk);
                let extra = serde_json::json!({
                    "code_file_path": url,
                    "code_language": language_name(&ext),
                    "code_file_type": classify_file_type(&url),
                    "code_is_test": is_test_path(&url),
                    "code_line_start": chunk.start_line,
                    "code_line_end": chunk.end_line,
                    "code_chunking_method": method,
                    "symbol_name": chunk.symbol_name,
                    "symbol_kind": chunk.symbol_kind_str(),
                    "symbol_extraction_status": status,
                });
                prepared.push(PreparedDoc {
                    url: format!("{url}#L{}-L{}", chunk.start_line, chunk.end_line),
                    domain: domain.clone(),
                    chunks: vec![chunk.text],
                    source_type: resolved_source_type.to_string(),
                    content_type: "text",
                    title: Some(url.clone()),
                    extra: Some(extra),
                    extractor_name: None,
                    structured: structured.clone(),
                });
            }
            continue;
        } else {
            // existing prose/markdown path (select_chunks) — unchanged
            let (chunks, content_type) = select_chunks(&url, raw).await;
            (chunks, content_type, None)
        };
```

> The exact interleaving with the existing `domain`/`structured` computation must be preserved; compute `domain` before this block. Keep the existing crawl-manifest branch (`url.starts_with("http")`) on the prose path so `changed==false` skip + structured reconstruction are untouched.

- [ ] **Step 3: Update local-embed tests for the new code path**

In `src/vector/ops/tei/prepare_tests.rs`, update `dir_embed_tags_code_and_prose_distinctly` to assert the richer payload, and keep `dir_embed_honors_crawl_manifest` unchanged:

```rust
#[tokio::test]
async fn dir_embed_code_file_gets_symbol_payload() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let root = temp_dir.path();
    tokio::fs::write(root.join("lib.rs"), "fn alpha() {}\n\nfn beta() {}\n")
        .await
        .expect("write lib.rs");

    let prepared = prepare_embed_docs(&cfg, &root.to_string_lossy(), &[], None)
        .await
        .expect("prepare docs");

    assert!(!prepared.is_empty());
    let rs = prepared.iter().find(|d| d.url.contains("lib.rs")).unwrap();
    assert_eq!(rs.content_type, "text");
    let extra = rs.extra.as_ref().expect("code payload");
    assert_eq!(extra["code_chunking_method"], "tree_sitter");
    assert_eq!(extra["code_file_type"], "source");
}
```

- [ ] **Step 4: Run local-embed tests**

Run: `cargo test --lib -- prepare_embed_docs dir_embed`
Expected: PASS (including `dir_embed_honors_crawl_manifest`).

- [ ] **Step 5: Commit**

```bash
git add src/vector/ops/tei/prepare.rs src/vector/ops/tei/prepare_tests.rs
git commit -m "feat(embed): local directory embed uses shared engine + symbol payload"
```

---

## Phase 5 — Docs, verification, version

### Task 7: Docs + CHANGELOG + version bump

**Files:**
- Modify: `src/ingest/CLAUDE.md`, `src/vector/CLAUDE.md`, `CHANGELOG.md`, `Cargo.toml`

- [ ] **Step 1: Update `src/ingest/CLAUDE.md`**

- In the GitLab section, replace the stale line "No file-level chunking strategy selection — all file content uses `chunk_text()`…" with: "File content uses the shared `file_ingest` engine: tree-sitter code chunking + `code_*`/`symbol_*` per-chunk metadata, same as GitHub."
- In Gitea + Generic Git sections, note they now produce code-aware chunks + symbol metadata via the shared engine.
- Add a short "Shared engine" note pointing at `src/vector/ops/file_ingest.rs`.

- [ ] **Step 2: Update `src/vector/CLAUDE.md`**

Under "Code Chunking", document `file_ingest::{collect_files, chunk_file}` as the single walker + chunk adapter used by all git providers and local embed; note `SelectionPolicy::{Allowlist, Permissive}`.

- [ ] **Step 3: Bump version + CHANGELOG**

In `Cargo.toml` bump `[package] version` by one minor (e.g. `6.x.0` → `6.(x+1).0` relative to whatever #187 set). Add a `CHANGELOG.md` entry under the new version:

```markdown
### Changed
- Unified all git providers (GitHub, GitLab, generic Git, Gitea) and local `embed <dir>`
  onto a shared `file_ingest` engine (one recursive walker + one code/prose chunk adapter).
- GitLab, generic Git, Gitea, and local code-file embeds now produce tree-sitter
  code chunks with `symbol_*` and `code_line_*`/`code_chunking_method` metadata
  (previously prose-only / GitHub-only). Fixes the generic-Git/Gitea prose-chunking
  bug and closes the GitLab symbol gap (axon_rust-wavn). No schema bump — existing
  v6 fields are now populated for more providers.
```

- [ ] **Step 4: Update `Cargo.lock`**

Run: `cargo update -p axon --precise <new-version>` or just `cargo check` (lock auto-updates the workspace member version).
Expected: `Cargo.lock` axon version matches `Cargo.toml`.

- [ ] **Step 5: Commit**

```bash
git add src/ingest/CLAUDE.md src/vector/CLAUDE.md CHANGELOG.md Cargo.toml Cargo.lock
git commit -m "docs: document shared file_ingest engine; bump version"
```

### Task 8: Full verification gate

- [ ] **Step 1: Full lib test suite**

Run: `cargo test --lib`
Expected: all pass (≥ baseline 2487).

- [ ] **Step 2: Contract + integration tests touched by payload changes**

Run: `cargo test --workspace --locked --features test-helpers -- --skip worker_e2e`
Expected: pass (covers `tests/mcp_contract_parity.rs`, payload index tests).

- [ ] **Step 3: Lint + format + monolith**

Run: `just verify`
Expected: fmt clean, clippy clean, monolith clean (new `file_ingest.rs` < 500 lines, all fns < 120).

- [ ] **Step 4: Live smoke (optional, needs Qdrant + TEI)**

```bash
just services-up
./scripts/axon ingest git:https://github.com/BurntSushi/ripgrep.git --wait true --collection engine_test
./scripts/axon query "argument parsing" --collection engine_test
# Confirm a code chunk carries symbol metadata:
./scripts/axon retrieve "https://github.com/.../blob/...#L..." --collection engine_test --json | grep -E "symbol_kind|code_chunking_method"
./scripts/axon embed ./src --wait true --collection engine_test
```

Expected: generic-git ingest produces `code_chunking_method: "tree_sitter"` chunks; local embed of `./src` produces symbol-bearing chunks.

---

## Self-Review Notes (already applied)

- **Spec coverage:** engine (T1), GitHub rewire (T2), GitLab symbols (T3–T4, closes `axon_rust-wavn`), generic+Gitea (T5), local embed (T6), docs/version (T7), verification (T8). All four "still needed" items covered.
- **No schema bump:** verified — `symbol_*`, `code_line_*`, `code_chunking_method` already exist in `build_git_payload` and are indexed (#187 v6). This plan only populates them for more providers.
- **Type consistency:** `chunk_file` returns `Vec<CodeChunk>` everywhere; `chunking_method(ext, &CodeChunk)` and `code_symbol_extraction_status(content, ext, &[CodeChunk])` signatures match `src/vector/ops/input/code.rs`. `SymbolKind::as_str` / `CodeChunk::symbol_kind_str` used consistently.
- **Sequencing risk:** this materially grows PR #187. If reviewers prefer, Phases 1–2, 3, and 4 are each independently committable and could ship as a stacked follow-up PR after #187 merges rather than inflating #187. Execution can stop after any phase with a green tree.
