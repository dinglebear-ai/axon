# Unify Code/File Ingestion Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract one shared `file_ingest` engine and route every git provider (GitHub, GitLab, generic Git, Gitea) and the local `embed <dir>` path through it, so tree-sitter code chunking and canonical `code_*`/`symbol_*` payload metadata are produced uniformly instead of via divergent per-provider copies.

**Architecture:** PR #192 (merged) built the symbol-aware `chunk_code_chunks()` → `Vec<CodeChunk>` API and the canonical `git_*`/`code_*`/`symbol_*` payload schema (v7), but only GitHub uses it. GitLab and generic Git still call the old `chunk_code()` → `Vec<String>` API (no symbol metadata). This plan creates `src/vector/ops/file_ingest.rs` (shared walker + `chunk_file` adapter returning `Vec<CodeChunk>`) and rewires all four providers and local embed onto it.

**Tech Stack:** Rust 2024, tokio, tree-sitter (`text_splitter::CodeSplitter`), `chunk_code_chunks` + `CodeChunk`, `build_git_payload`, Qdrant payload schema v7.

**Bead:** `axon_rust-rcbe` (closes `axon_rust-wavn` in Phase 2)

**Supersedes:** `docs/superpowers/plans/2026-06-08-unify-code-file-ingestion-engine.md` (same scope; corrected for `CodeChunk.symbol` field rename and `PreparedDoc.chunk_extra` addition that landed post-plan).

---

## Current State (as of 2026-06-10)

| Provider | Walker | Chunking API | Symbol metadata |
|----------|--------|-------------|-----------------|
| GitHub | own `collect_indexable_files` in `prepare.rs` | `chunk_code_chunks` (Vec<CodeChunk>) ✅ | yes, via `chunk_extra` ✅ |
| GitLab | `collect_repo_files` (shared, `git_files.rs`) ✅ | `chunk_code` (Vec<String>) ❌ | none ❌ |
| generic Git | `collect_repo_files` (shared, `git_files.rs`) ✅ | `chunk_code` (Vec<String>) ❌ | none ❌ |
| Gitea | delegates to generic Git path ✅ | inherits ❌ | none ❌ |
| local `embed <dir>` | own logic in `tei/prepare.rs` | `chunk_code` via `select_chunks` ❌ | none ❌ |

Phases 1–2 fix GitHub (cosmetic dedup) and GitLab. Phase 3 fixes generic Git + Gitea. Phase 4 fixes local embed.

## Preconditions

Both dependencies are already merged to `main`:
- PR #188 (`feat: recursive + AST-aware local directory embed`) — `src/vector/ops/input/select.rs` exists ✅
- PR #192 (`feat(ingest): symbol-aware GitHub code chunking`) — `chunk_code_chunks`, `CodeChunk`, `code_symbol_extraction_status` exist ✅

Confirm before starting:

```bash
test -f src/vector/ops/input/select.rs && echo "select.rs OK" || echo "MISSING"
cargo check --bin axon
```

Expected: `select.rs OK`, then `Finished`.

---

## File Structure

| File | Status | Responsibility |
|------|--------|----------------|
| `src/vector/ops/file_ingest.rs` | **Create** | Shared engine: `SelectionPolicy`, `collect_files()` (async walker), `chunk_file()` (`Vec<CodeChunk>`), `chunking_method()`, prose→CodeChunk adapter helpers |
| `src/vector/ops/file_ingest_tests.rs` | **Create** | Sidecar tests for the engine |
| `src/vector/ops.rs` | **Modify** | Add `pub mod file_ingest;` |
| `src/ingest/github/files/prepare.rs` | **Modify** | Rewire walker + chunk helpers onto shared engine |
| `src/ingest/gitlab/embed.rs` | **Modify** | Add `gitlab_file_chunk_payload()` per-chunk builder |
| `src/ingest/gitlab/files.rs` | **Modify** | Switch chunking from `chunk_code` → `chunk_file` + per-chunk docs with symbol metadata |
| `src/ingest/generic_git.rs` | **Modify** | Replace `file_doc` (one doc/file) with `file_docs` (one doc/chunk, symbol metadata) |
| `src/vector/ops/tei/prepare.rs` | **Modify** | Replace `select_chunks` code path with `chunk_file` + per-chunk `PreparedDoc` with `code_*`/`symbol_*` payload |
| `src/ingest/CLAUDE.md`, `src/vector/CLAUDE.md` | **Modify** | Update to reflect shared engine |
| `CHANGELOG.md`, `Cargo.toml` | **Modify** | Version bump + entry |

**No `PAYLOAD_SCHEMA_VERSION` bump:** all target fields (`symbol_*`, `code_line_*`, `code_chunking_method`) already exist in schema v7 from PR #192. This plan only *populates* them for more providers.

---

## Phase 1 — Shared engine + GitHub rewire

### Task 1: Create `src/vector/ops/file_ingest.rs`

**Files:**
- Create: `src/vector/ops/file_ingest.rs`
- Create: `src/vector/ops/file_ingest_tests.rs`
- Modify: `src/vector/ops.rs`

- [ ] **Step 1: Declare the module**

In `src/vector/ops.rs`, add alongside the other `pub mod` declarations:

```rust
pub mod file_ingest;
```

Verify: `grep -n "pub mod file_ingest;" src/vector/ops.rs` → one match.

- [ ] **Step 2: Write the engine**

Create `src/vector/ops/file_ingest.rs`:

```rust
//! Shared filesystem → chunked-document engine.
//!
//! One recursive walker + one chunk-selection adapter used by every git
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
/// Resilience: unreadable root is a hard error; unreadable subdirectory is
/// logged and skipped. Pruned directories (`.git`, `node_modules`, `target`,
/// …) are never descended into. Symlinks are skipped. Returned paths are
/// sorted for deterministic ordering.
pub async fn collect_files(
    root: &Path,
    policy: SelectionPolicy,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut at_root = true;
    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(e) => e,
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
                Ok(Some(e)) => e,
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
            let Ok(ft) = entry.file_type().await else {
                log_warn(&format!(
                    "command=ingest skip_unknown_type path={}",
                    path.display()
                ));
                continue;
            };
            if ft.is_dir() {
                if !select::is_pruned_dir(name) {
                    stack.push(path);
                }
            } else if ft.is_file() && include_file(&path, root, policy) {
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
            !select::is_binary_ext(
                crate::vector::ops::input::classify::path_extension(name),
            )
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

/// Chunk one file's content into `CodeChunk`s using tree-sitter when a
/// grammar exists for `ext`, otherwise adapting prose chunks to `CodeChunk`.
/// CPU-bound — callers should wrap in `spawn_blocking`.
pub fn chunk_file(content: &str, ext: &str) -> Vec<CodeChunk> {
    chunk_code_chunks(content, ext).unwrap_or_else(|| text_chunks(content))
}

/// Report the chunking method: `"tree_sitter"` when the extension has a
/// grammar or a symbol was found, otherwise `"prose"`.
pub fn chunking_method(ext: &str, chunk: &CodeChunk) -> &'static str {
    if chunk.symbol.is_some() || supports_tree_sitter_chunking(ext) {
        "tree_sitter"
    } else {
        "prose"
    }
}

/// Adapt prose chunks (no symbol info) to `CodeChunk` by finding byte offsets
/// in the original text and computing line numbers.
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
                symbol: None,
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
    tokio::fs::write(root.join("img.png"), b"\x89PNG").await.unwrap();
    tokio::fs::create_dir_all(root.join("node_modules")).await.unwrap();
    tokio::fs::write(root.join("node_modules/x.js"), "1").await.unwrap();

    let files = collect_files(root, SelectionPolicy::Permissive).await.unwrap();
    let names: Vec<_> = files.iter().map(|p| p.to_string_lossy().to_string()).collect();
    assert_eq!(files.len(), 2, "expected 2 files, got: {names:?}");
    assert!(names.iter().any(|n| n.ends_with("a/b/c.rs")));
    assert!(names.iter().any(|n| n.ends_with("r.md")));
    assert!(!names.iter().any(|n| n.ends_with("img.png")));
    assert!(!names.iter().any(|n| n.contains("node_modules")));
}

#[tokio::test]
async fn allowlist_excludes_source_when_include_source_false() {
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
        chunks.iter().any(|c| c.symbol.is_some()),
        "expected at least one symbol-bearing chunk"
    );
    assert_eq!(chunking_method("rs", &chunks[0]), "tree_sitter");
}

#[test]
fn chunk_file_falls_back_to_prose_for_unknown_ext() {
    let text = "plain prose ".repeat(400);
    let chunks = chunk_file(&text, "md");
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|c| c.symbol.is_none()));
    assert_eq!(chunking_method("md", &chunks[0]), "prose");
}
```

- [ ] **Step 4: Run the engine tests**

```bash
cargo test --lib -- file_ingest
```

Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add src/vector/ops.rs src/vector/ops/file_ingest.rs src/vector/ops/file_ingest_tests.rs
git commit -m "feat(ingest): shared file_ingest engine (walker + chunk_file)"
```

---

### Task 2: Rewire GitHub onto the shared engine

**Files:**
- Modify: `src/ingest/github/files/prepare.rs`

The goal is behavior-preserving: delete the local walker and chunk helpers and call the shared engine instead. GitHub's existing golden tests (`prepare_tests.rs`) must stay green.

- [ ] **Step 1: Replace imports in `prepare.rs`**

Replace the existing `use` block for chunking/walker utilities with:

```rust
use crate::vector::ops::file_ingest::{SelectionPolicy, chunk_file, chunking_method, collect_files};
use crate::vector::ops::input::classify::{classify_file_type, is_test_path, language_name, path_extension};
use crate::vector::ops::input::code::{CodeChunk, code_symbol_extraction_status};
```

- [ ] **Step 2: Replace `collect_indexable_files`**

Delete the `collect_indexable_files` function and replace with a thin wrapper that returns repo-relative strings (downstream callers still expect `Vec<String>`):

```rust
pub(super) async fn collect_indexable_files(
    root: &Path,
    include_source: bool,
) -> Result<Vec<String>> {
    let abs = collect_files(root, SelectionPolicy::Allowlist { include_source })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(abs
        .into_iter()
        .filter_map(|p| {
            p.strip_prefix(root)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .collect())
}
```

- [ ] **Step 3: Replace the `spawn_blocking` chunk block in `read_file_embed_docs`**

In `read_file_embed_docs`, find the block that calls the local chunk helpers and replace with a call to `chunk_file`:

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

Delete the now-moved helpers from `prepare.rs`: `next_search_start`, `code_or_text_chunks`, `text_chunks`, `chunking_method`, `line_for_byte`. (`chunking_method` and `code_symbol_extraction_status` are now imported from the engine and `code.rs`.)

- [ ] **Step 4: Run the existing GitHub golden tests**

```bash
cargo test --lib -- ingest::github::files
```

Expected: PASS (v7 payload + symbol assertions in `prepare_tests.rs` and `meta_tests.rs` are unchanged).

- [ ] **Step 5: Commit**

```bash
git add src/ingest/github/files/prepare.rs
git commit -m "refactor(ingest): GitHub file ingest uses shared file_ingest engine"
```

---

## Phase 2 — GitLab: code-aware chunking + symbols (closes `axon_rust-wavn`)

GitLab already uses `collect_repo_files` from `git_files.rs` for the walk. The remaining gap is chunking: it calls `chunk_code()` → `Vec<String>` (no symbols). Phase 2 switches it to `chunk_file()` → `Vec<CodeChunk>` and adds per-chunk metadata.

### Task 3: Add `gitlab_file_chunk_payload`

**Files:**
- Modify: `src/ingest/gitlab/embed.rs`
- Create: `src/ingest/gitlab/embed_tests.rs` (if absent; else append)

- [ ] **Step 1: Wire the test sidecar (if not yet present)**

Check `src/ingest/gitlab/embed.rs` for an existing `#[cfg(test)]` path declaration. If absent, append to `embed.rs`:

```rust
#[cfg(test)]
#[path = "embed_tests.rs"]
mod tests;
```

- [ ] **Step 2: Write a failing test**

Append to `src/ingest/gitlab/embed_tests.rs` (create if absent):

```rust
#[test]
fn gitlab_file_chunk_payload_sets_code_and_symbol_fields() {
    use crate::ingest::gitlab::types::{GitLabProject, GitLabTarget};
    use crate::vector::ops::input::code::{CodeChunk, Symbol, SymbolKind};

    let target = GitLabTarget {
        host: "gitlab.com".into(),
        namespace_path: "group/project".into(),
        project: "project".into(),
        web_url: "https://gitlab.com/group/project".into(),
        clone_url: "https://gitlab.com/group/project.git".into(),
        api_base: "https://gitlab.com/api/v4".into(),
        encoded_project_path: "group%2Fproject".into(),
    };
    let project = GitLabProject {
        path_with_namespace: "group/project".into(),
        name: "project".into(),
        description: None,
        default_branch: Some("main".into()),
        web_url: "https://gitlab.com/group/project".into(),
        visibility: Some("public".into()),
        star_count: None,
        forks_count: None,
        open_issues_count: None,
        issues_enabled: Some(true),
        merge_requests_enabled: Some(true),
        wiki_enabled: Some(false),
        last_activity_at: None,
    };
    let chunk = CodeChunk {
        text: "fn x() {}".into(),
        byte_start: 0,
        byte_end: 9,
        start_line: 10,
        end_line: 12,
        declaration_start_line: 10,
        declaration_end_line: 12,
        symbol: Some(Symbol {
            name: Some("x".into()),
            kind: SymbolKind::Function,
        }),
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

- [ ] **Step 3: Run — expect failure**

```bash
cargo test --lib -- gitlab_file_chunk_payload
```

Expected: FAIL ("cannot find function `gitlab_file_chunk_payload`").

- [ ] **Step 4: Implement `gitlab_file_chunk_payload` in `embed.rs`**

Add to `src/ingest/gitlab/embed.rs` (reuse the `build_git_payload`, `language_name`, `classify_file_type`, `is_test_path`, `path_extension` imports already in scope):

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
    use crate::ingest::git_payload::ContentKind;
    let owner = {
        let path = &target.namespace_path;
        path.rfind('/').map(|i| path[..i].to_string())
    };
    build_git_payload(&GitPayload {
        provider: "gitlab".to_string(),
        host: target.host.clone(),
        owner,
        repo: target.project.clone(),
        content_kind: ContentKind::File,
        branch: Some(branch.to_string()),
        default_branch: project.default_branch.clone(),
        file_path: Some(rel.to_string()),
        file_language: Some(language_name(path_extension(rel)).to_string()),
        file_type: Some(classify_file_type(rel).to_string()),
        file_is_test: Some(is_test_path(rel)),
        line_start: Some(chunk.start_line),
        line_end: Some(chunk.end_line),
        chunking_method: Some(chunking_method.to_string()),
        symbol_name: chunk.symbol_name().map(str::to_string),
        symbol_kind: chunk.symbol_kind_str().map(str::to_string),
        symbol_extraction_status: Some(symbol_status.to_string()),
        meta: Some(serde_json::json!({
            "namespace_path": target.namespace_path,
            "visibility": project.visibility,
            "last_activity_at": project.last_activity_at,
        })),
        ..GitPayload::default()
    })
}
```

- [ ] **Step 5: Run test — expect pass**

```bash
cargo test --lib -- gitlab_file_chunk_payload
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/ingest/gitlab/embed.rs src/ingest/gitlab/embed_tests.rs
git commit -m "feat(ingest): canonical per-chunk GitLab file payload with symbols"
```

---

### Task 4: Route GitLab `embed_files` through `chunk_file`

**Files:**
- Modify: `src/ingest/gitlab/files.rs`

The walk already uses `collect_repo_files`. This task replaces the `chunk_code` → `Vec<String>` path with `chunk_file` → `Vec<CodeChunk>` and emits one `PreparedDoc` per chunk via the new payload builder.

- [ ] **Step 1: Update imports in `files.rs`**

Add to the import block:

```rust
use crate::vector::ops::file_ingest::{chunk_file, chunking_method};
use crate::vector::ops::input::code::code_symbol_extraction_status;
use super::embed::gitlab_file_chunk_payload;
```

Remove the now-unused `chunk_code`, `chunk_text` imports from this file.

- [ ] **Step 2: Replace the per-file chunk loop**

In `embed_files`, replace the block from the `spawn_blocking` call through the `docs.push(PreparedDoc::ingest(...))` call with:

```rust
        let ext = Path::new(&rel)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let (chunks, content, ext) = match tokio::task::spawn_blocking(move || {
            let chunks = chunk_file(&content, &ext);
            (chunks, content, ext)
        })
        .await
        {
            Ok(result) => result,
            Err(e) => {
                log_warn(&format!(
                    "command=ingest_gitlab chunk_panicked path={rel} err={e}"
                ));
                continue;
            }
        };
        if chunks.is_empty() {
            continue;
        }
        let symbol_status = code_symbol_extraction_status(&content, &ext, &chunks);
        for chunk in chunks {
            let method = chunking_method(&ext, &chunk);
            docs.push(PreparedDoc::ingest(
                format!(
                    "{}/-/blob/{}/{}#L{}-L{}",
                    target.web_url, branch, rel, chunk.start_line, chunk.end_line
                ),
                target.host.clone(),
                vec![chunk.text.clone()],
                "gitlab",
                Some(rel.clone()),
                Some(gitlab_file_chunk_payload(
                    target, project, &rel, branch, &chunk, method, symbol_status,
                )),
            ));
        }
```

- [ ] **Step 3: Verify compile**

```bash
cargo check --bin axon
```

Expected: `Finished`.

- [ ] **Step 4: Run GitLab unit tests**

```bash
cargo test --lib -- gitlab
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ingest/gitlab/files.rs
git commit -m "feat(ingest): GitLab files code-aware chunking + per-chunk symbol metadata (closes axon_rust-wavn)"
```

---

## Phase 3 — generic Git + Gitea: code-aware chunking + symbols

### Task 5: Convert `file_doc` → `file_docs` in `generic_git.rs`

The walk already uses `collect_repo_files`. This task replaces `file_doc` (one `PreparedDoc` per file, `Vec<String>` chunks, no symbols) with `file_docs` (one `PreparedDoc` per chunk, `Vec<CodeChunk>`, full symbol metadata).

**Files:**
- Modify: `src/ingest/generic_git.rs`
- Modify: `src/ingest/generic_git_tests.rs`

- [ ] **Step 1: Write a failing test**

Append to `src/ingest/generic_git_tests.rs`:

```rust
#[tokio::test]
async fn file_docs_chunks_rust_as_code_with_symbols() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("lib.rs"), "fn alpha() {}\n\nfn beta() {}\n").unwrap();
    let target = crate::ingest::generic_git::GenericGitTarget {
        host: "example.com".into(),
        name: "repo".into(),
        clone_url: "https://example.com/r.git".into(),
        web_url: "https://example.com/r".into(),
    };
    let docs = super::file_docs(root, &target, "main", root.join("lib.rs"), "git", "git")
        .await
        .unwrap();
    assert!(!docs.is_empty(), "expected at least one doc");
    let extra = docs[0].extra.as_ref().expect("expected payload");
    assert_eq!(extra["code_chunking_method"], "tree_sitter");
    assert_eq!(extra["code_file_type"], "source");
    assert!(
        docs.iter()
            .any(|d| d.extra.as_ref().unwrap()["symbol_kind"] == "function"),
        "expected at least one chunk with symbol_kind=function"
    );
}
```

- [ ] **Step 2: Run — expect failure**

```bash
cargo test --lib -- file_docs_chunks_rust
```

Expected: FAIL ("no function named `file_docs`").

- [ ] **Step 3: Replace `file_doc` with `file_docs`**

Add to the import block in `generic_git.rs`:

```rust
use crate::vector::ops::file_ingest::{chunk_file, chunking_method};
use crate::vector::ops::input::code::code_symbol_extraction_status;
```

Replace the `file_doc` function entirely with `file_docs`:

```rust
pub(crate) async fn file_docs(
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

    match tokio::fs::metadata(&file).await {
        Ok(meta) if meta.len() > MAX_INGEST_FILE_BYTES => {
            log_warn(&format!(
                "command=ingest_git skip_large_file path={rel} size_bytes={}",
                meta.len()
            ));
            return Ok(Vec::new());
        }
        Err(e) => {
            log_warn(&format!(
                "command=ingest_git stat_failed path={rel} err={e}"
            ));
            return Ok(Vec::new());
        }
        _ => {}
    }

    let bytes = match tokio::fs::read(&file).await {
        Ok(b) => b,
        Err(e) => {
            log_warn(&format!(
                "command=ingest_git read_failed path={rel} err={e}"
            ));
            return Ok(Vec::new());
        }
    };
    let content = match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => {
            log_warn(&format!("command=ingest_git skip_non_utf8 path={rel}"));
            return Ok(Vec::new());
        }
    };

    let ext = path_extension(&rel).to_ascii_lowercase();
    let ext_for_block = ext.clone();
    let (chunks, content) = tokio::task::spawn_blocking(move || {
        let chunks = chunk_file(&content, &ext_for_block);
        (chunks, content)
    })
    .await
    .map_err(|e| anyhow!("chunk_file panicked: {e}"))?;
    if chunks.is_empty() {
        return Ok(Vec::new());
    }

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
            content_kind: ContentKind::File,
            branch: Some(branch.to_string()),
            file_path: Some(rel.clone()),
            file_language: Some(lang.clone()),
            file_type: Some(ftype.clone()),
            file_is_test: Some(is_test),
            line_start: Some(chunk.start_line),
            line_end: Some(chunk.end_line),
            chunking_method: Some(method.to_string()),
            symbol_name: chunk.symbol_name().map(str::to_string),
            symbol_kind: chunk.symbol_kind_str().map(str::to_string),
            symbol_extraction_status: Some(symbol_status.to_string()),
            meta: Some(serde_json::json!({ "clone_url": target.clone_url })),
            ..GitPayload::default()
        });
        docs.push(PreparedDoc::ingest(
            format!(
                "{}#{}:{}#L{}-L{}",
                target.web_url, branch, rel, chunk.start_line, chunk.end_line
            ),
            target.host.clone(),
            vec![chunk.text],
            source_type,
            Some(rel.clone()),
            Some(extra),
        ));
    }
    Ok(docs)
}
```

- [ ] **Step 4: Update the caller in `ingest_git_repository`**

Replace the loop that calls `file_doc` with one that calls `file_docs`:

```rust
    for (index, file) in files.into_iter().enumerate() {
        let mut file_result =
            file_docs(tmp.path(), &target, &branch, file, source_type, provider).await?;
        docs.append(&mut file_result);
        if (index + 1) % 25 == 0 || index + 1 == total {
            reporter
                .report(serde_json::json!({"files_done": index + 1, "files_total": total}))
                .await;
        }
    }
```

- [ ] **Step 5: Run the new test**

```bash
cargo test --lib -- file_docs_chunks_rust
```

Expected: PASS.

- [ ] **Step 6: Confirm Gitea inherits (no code change needed)**

Gitea routes file ingest through `ingest_git_repository`. Run its tests:

```bash
cargo test --lib -- gitea generic_git
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/ingest/generic_git.rs src/ingest/generic_git_tests.rs
git commit -m "feat(ingest): generic Git + Gitea code-aware chunking with symbol metadata"
```

---

## Phase 4 — Local `embed <dir>` onto the engine

### Task 6: Route local directory embed through `chunk_file`

**Files:**
- Modify: `src/vector/ops/tei/prepare.rs`
- Modify: `src/vector/ops/tei/prepare_tests.rs` (or create sidecar if needed)

Local code files currently go through `select_chunks` which calls `chunk_code()` → `Vec<String>`. This task adds per-chunk `PreparedDoc`s with `code_*`/`symbol_*` payload for local code files while leaving the crawl-output manifest path (http URLs) untouched.

- [ ] **Step 1: Add imports to `prepare.rs`**

```rust
use crate::vector::ops::file_ingest::{chunk_file, chunking_method};
use crate::vector::ops::input::classify::{classify_file_type, is_test_path, language_name, path_extension};
use crate::vector::ops::input::code::code_symbol_extraction_status;
use crate::vector::ops::input::select;
```

- [ ] **Step 2: Write a failing test**

In the prepare tests file (check existing sidecar path from `prepare.rs`), add:

```rust
#[tokio::test]
async fn dir_embed_code_file_gets_symbol_payload() {
    use crate::core::config::Config;
    let cfg = Config::default_minimal();
    let temp_dir = tempfile::TempDir::new().expect("tempdir");
    let root = temp_dir.path();
    tokio::fs::write(root.join("lib.rs"), "fn alpha() {}\n\nfn beta() {}\n")
        .await
        .expect("write");

    let prepared = prepare_embed_docs(&cfg, &root.to_string_lossy(), &[], None)
        .await
        .expect("prepare docs");

    let rs = prepared
        .iter()
        .find(|d| d.url.contains("lib.rs"))
        .expect("expected at least one lib.rs doc");
    assert_eq!(rs.content_type, "text");
    let extra = rs.extra.as_ref().expect("expected code payload");
    assert_eq!(extra["code_chunking_method"], "tree_sitter");
    assert_eq!(extra["code_file_type"], "source");
}
```

- [ ] **Step 3: Run — expect failure**

```bash
cargo test --lib -- dir_embed_code_file_gets_symbol_payload
```

Expected: FAIL (the assertion on `code_chunking_method` fails — the current path doesn't set it).

- [ ] **Step 4: Replace the local-code branch of `prepare_embed_docs`**

In `prepare_embed_docs` in `prepare.rs`, find the block that calls `select_chunks` and the subsequent `prepared.push(PreparedDoc { ... })`. Replace it with a branch that handles local code files separately:

```rust
        let is_local_code = !url.starts_with("http") && select::should_chunk_as_code(&url);
        if is_local_code {
            let ext = path_extension(&url).to_ascii_lowercase();
            let ext_for_block = ext.clone();
            let raw_for_block = raw.clone();
            let code_chunks = match tokio::task::spawn_blocking(move || {
                chunk_file(&raw_for_block, &ext_for_block)
            })
            .await
            {
                Ok(chunks) => chunks,
                Err(e) => {
                    log_warn(&format!(
                        "command=embed code_chunk_join_error url={url} err={e}"
                    ));
                    Vec::new()
                }
            };
            if code_chunks.is_empty() {
                skipped_empty += 1;
                continue;
            }
            let symbol_status = code_symbol_extraction_status(&raw, &ext, &code_chunks);
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
                    "symbol_name": chunk.symbol_name(),
                    "symbol_kind": chunk.symbol_kind_str(),
                    "symbol_extraction_status": symbol_status,
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
                    chunk_extra: Vec::new(),
                });
            }
            continue;
        }
        // --- existing prose/markdown path unchanged below ---
        let (chunks, content_type) = select_chunks(&url, raw).await;
```

> Keep `domain` computation and the crawl-manifest reconstruction (`structured`) above this block — those variables are needed.

- [ ] **Step 5: Run the test**

```bash
cargo test --lib -- dir_embed_code_file_gets_symbol_payload
```

Expected: PASS.

- [ ] **Step 6: Run the full prepare suite (including the crawl-manifest test)**

```bash
cargo test --lib -- prepare_embed_docs dir_embed
```

Expected: all PASS (including `dir_embed_honors_crawl_manifest` — that path is unchanged).

- [ ] **Step 7: Commit**

```bash
git add src/vector/ops/tei/prepare.rs
git commit -m "feat(embed): local directory embed uses shared engine + symbol payload"
```

---

## Phase 5 — Docs, verification, version

### Task 7: Update docs and bump version

**Files:**
- Modify: `src/ingest/CLAUDE.md`
- Modify: `src/vector/CLAUDE.md`
- Modify: `CHANGELOG.md`
- Modify: `Cargo.toml`, `apps/web/package.json`, `apps/web/openapi/axon.json`, `README.md`

- [ ] **Step 1: Update `src/ingest/CLAUDE.md`**

  - In the GitLab section: replace the stale "No file-level chunking strategy selection — all file content uses `chunk_text()`" note with: "File content uses the shared `file_ingest` engine — tree-sitter code chunking + `code_*`/`symbol_*` per-chunk metadata, same as GitHub."
  - In the Gitea and Generic Git sections: note they now produce code-aware chunks with symbol metadata via the shared engine.
  - Add a short "Shared engine" cross-reference: "All git providers and local `embed <dir>` share `src/vector/ops/file_ingest.rs` (`SelectionPolicy`, `collect_files`, `chunk_file`, `chunking_method`)."

- [ ] **Step 2: Update `src/vector/CLAUDE.md`**

Under the "Code Chunking" section, add a note: "`file_ingest::{collect_files, chunk_file}` is the single walker + chunk adapter used by all git providers and local embed — see `SelectionPolicy::{Allowlist, Permissive}`."

- [ ] **Step 3: Bump version in all files**

Current version: `5.8.1`. This is a `feat` commit series → bump to `5.9.0`.

Update `Cargo.toml`:
```toml
version = "5.9.0"
```

Update `apps/web/package.json`:
```json
"version": "5.9.0"
```

Update `apps/web/openapi/axon.json` — find the `"version": "5.8.1"` field and change to `"5.9.0"`.

Update `README.md` — find and update any version badge or header showing `5.8.1` → `5.9.0`.

- [ ] **Step 4: Add CHANGELOG entry**

Under a new `## [5.9.0]` heading in `CHANGELOG.md`:

```markdown
## [5.9.0] - 2026-06-10

### Changed
- Unified all git providers (GitHub, GitLab, generic Git, Gitea) and local
  `embed <dir>` onto a shared `file_ingest` engine — one recursive walker +
  one code/prose chunk adapter.
- GitLab, generic Git, Gitea, and local code-file embeds now produce
  tree-sitter code chunks with `symbol_*`, `code_line_*`, and
  `code_chunking_method` payload metadata (previously prose-only /
  GitHub-only). Fixes the generic-Git/Gitea prose-chunking regression and
  closes the GitLab symbol gap (`axon_rust-wavn`).
- No schema version bump — all target fields already exist in payload schema
  v7 from PR #192. This release populates them for the remaining providers.
```

- [ ] **Step 5: Commit**

```bash
git add src/ingest/CLAUDE.md src/vector/CLAUDE.md CHANGELOG.md Cargo.toml \
        apps/web/package.json apps/web/openapi/axon.json README.md Cargo.lock
git commit -m "docs: shared file_ingest engine; bump to 5.9.0"
```

---

### Task 8: Full verification gate

- [ ] **Step 1: Full lib test suite**

```bash
cargo test --lib
```

Expected: all pass (≥ baseline 2487).

- [ ] **Step 2: Lint + format + monolith**

```bash
just verify
```

Expected: `cargo fmt` clean, `cargo clippy` clean, monolith clean (`file_ingest.rs` < 500 lines, all functions < 120 lines).

- [ ] **Step 3: Integration tests**

```bash
cargo test --workspace --locked -- --skip worker_e2e
```

Expected: pass (covers `tests/mcp_contract_parity.rs` and payload-index tests).

- [ ] **Step 4: Live smoke test (requires Qdrant + TEI)**

```bash
just services-up
./scripts/axon ingest git:https://github.com/BurntSushi/ripgrep.git \
    --wait true --collection engine_smoke
./scripts/axon query "argument parsing" --collection engine_smoke
# Confirm a code chunk carries symbol metadata:
./scripts/axon retrieve "$(./scripts/axon sources --collection engine_smoke --json | \
    jq -r '.sources[0].url')" --collection engine_smoke --json \
    | grep -E "symbol_kind|code_chunking_method"
./scripts/axon embed ./src --wait true --collection engine_smoke
```

Expected: generic-git ingest produces `code_chunking_method: "tree_sitter"` chunks with `symbol_kind` values; local embed of `./src` produces symbol-bearing chunks.

---

## Self-Review

**Spec coverage:**
- ✅ Shared engine (Task 1)
- ✅ GitHub rewire — behavior-preserving dedup (Task 2)
- ✅ GitLab symbols — closes `axon_rust-wavn` (Tasks 3–4)
- ✅ Generic Git + Gitea symbols (Task 5)
- ✅ Local embed symbols (Task 6)
- ✅ Docs + version bump (Task 7)
- ✅ Verification gate (Task 8)

**Corrections vs the 2026-06-08 plan (applied here):**
1. `CodeChunk.symbol` field (not `symbol_name`/`symbol_kind`) — all struct literals updated.
2. `chunk.symbol.is_some()` in `chunking_method` (not `chunk.symbol_kind.is_some()`).
3. Symbol access via methods: `chunk.symbol_name()`, `chunk.symbol_kind_str()` (not field access).
4. `GitPayload.content_kind: ContentKind::File` (not `"file"` string literal).
5. `PreparedDoc` struct literal in Task 6 includes `chunk_extra: Vec::new()`.
6. Tasks 4 and 5 use `PreparedDoc::ingest()` constructor (handles `chunk_extra` automatically).
7. Preconditions: PRs #188 and #192 are already merged — no branch switching required.
8. Task 4 walker note: GitLab already uses `collect_repo_files`; only the chunking path changes.

**Corrections from 2026-06-10 plan review:**
9. Task 3 test `GitLabProject` literal: removed nonexistent `id` field; added all 8 missing fields (`path_with_namespace`, `name`, `description`, `web_url`, `star_count`, `forks_count`, `open_issues_count`); corrected `issues_enabled`/`merge_requests_enabled`/`wiki_enabled` from `bool` → `Option<bool>` (matching the actual `Deserialize` struct).
10. Task 4 `spawn_blocking` binding: was binding 3-tuple return to 2-tuple `let (chunks, content) =` then re-destructuring `chunks` as tuple — compile error. Fixed to `let (chunks, content, ext) = match ... { Ok(result) => result, ... };`.
