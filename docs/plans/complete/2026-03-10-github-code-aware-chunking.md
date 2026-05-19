# GitHub Code-Aware Chunking + Source by Default — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make GitHub ingestion index source code by default using tree-sitter AST-aware chunking, with unified metadata payloads across all GitHub chunk types.

**Architecture:** Six components split across three layers: (1) a pure code chunker in `vector/ops/input/` that maps file extensions to tree-sitter grammars, (2) an embedding pipeline function in `tei.rs` that tries code chunking first with fallback to prose, (3) a unified payload builder and default-flip in the GitHub ingest layer, and (4) refresh schedule extensions for GitHub repos. Each component is independently testable and the layers communicate through existing interfaces.

**Tech Stack:** `text-splitter` 0.29 (with `code` feature), `tree-sitter-{rust,python,javascript,typescript,go,bash}` grammar crates, existing `octocrab`/`reqwest`/`sqlx` infrastructure.

**Spec:** `docs/superpowers/specs/2026-03-10-github-code-aware-chunking-design.md`

---

## Scope Check

The spec covers 6 components. Components 1–5 are tightly coupled (code chunker → embed pipeline → payload → CLI flag → files.rs integration) and must ship together. Component 6 (refresh scheduling) is independent but small enough to include in one plan. The plan is structured so Component 6 can be deferred without affecting Components 1–5.

---

## File Structure

### New Files

| File | Responsibility |
|------|---------------|
| `crates/vector/ops/input/code.rs` | `chunk_code()` + `language_for_extension()` — pure functions, no I/O |
| `crates/vector/ops/input/classify.rs` | `classify_file_type()`, `language_name()`, `is_test_path()` — pure classification heuristics |

### Modified Files

| File | Changes |
|------|---------|
| `Cargo.toml` | Add `text-splitter`, 6 grammar crates |
| `crates/vector/ops/input.rs` → `crates/vector/ops/input.rs` | Convert to module root: add `mod code; mod classify;` declarations, keep `chunk_text()` + `url_lookup_candidates()` in place |
| `crates/vector/ops/tei.rs` | Add `embed_code_with_metadata()` that tries `chunk_code()` → fallback `chunk_text()` |
| `crates/ingest/github/meta.rs` | Replace 3 per-type builders with unified `GitHubPayloadParams` + `build_github_payload()` |
| `crates/ingest/github.rs` | Extract common fields from `repo_info`, pass to all sub-tasks |
| `crates/ingest/github/files.rs` | Switch to `embed_code_with_metadata()`, pass file metadata |
| `crates/ingest/github/issues.rs` | Switch to `build_github_payload()` with common fields |
| `crates/ingest/github/wiki.rs` | Switch to `build_github_payload()` with common fields |
| `crates/core/config/cli.rs` | Add `--no-source` flag |
| `crates/core/config/types/subconfigs.rs` | Change `github_include_source` default to `true` |
| `crates/core/config/types/config_impls.rs` | Change `github_include_source` default to `true` |
| `crates/core/config/parse/build_config.rs` | Wire `--no-source` → `github_include_source = false` |
| `crates/jobs/refresh.rs` | Add `source_type TEXT`, `target TEXT` columns to schema |
| `crates/jobs/refresh/schedule.rs` | Add `source_type`/`target` fields to `RefreshScheduleCreate`, handle GitHub schedule ticks |

### Key Constraint: Module Conversion for `input.rs`

`input.rs` currently lives at `crates/vector/ops/input.rs` as a flat file. To add `input/code.rs` and `input/classify.rs`, we need to convert it to a module directory. Per the project's "never use mod.rs" convention:

- `crates/vector/ops/input.rs` stays as the module root (it already is — declared as `pub mod input;` in `ops.rs`)
- New submodules: `crates/vector/ops/input/code.rs`, `crates/vector/ops/input/classify.rs`
- Add `pub mod code;` and `pub mod classify;` to `input.rs`

This works because Rust 2018+ resolves `mod code;` inside `input.rs` by looking for `input/code.rs`.

---

## Chunk 1: Code Chunker Module + Dependencies

### Task 1: Add Dependencies to Cargo.toml

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add crate dependencies**

Add to `[dependencies]` in `Cargo.toml` (alphabetical insertion):

```toml
text-splitter = { version = "0.29", features = ["code"] }
tree-sitter-bash = "0.23"
tree-sitter-go = "0.23"
tree-sitter-javascript = "0.23"
tree-sitter-language = "0.1"
tree-sitter-python = "0.23"
tree-sitter-rust = "0.24"
tree-sitter-typescript = "0.23"
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: successful compilation (may take 30-60s for grammar crate C compilation on first build)

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "$(cat <<'EOF'
feat(deps): add text-splitter + tree-sitter grammar crates

For AST-aware code chunking in GitHub ingestion.
Grammars: Rust, Python, JavaScript, TypeScript, Go, Bash.
EOF
)"
```

---

### Task 2: Create File Classification Module

**Files:**
- Create: `crates/vector/ops/input/classify.rs`
- Modify: `crates/vector/ops/input.rs` (add `pub mod classify;`)

- [ ] **Step 1: Write failing tests for `classify_file_type()`**

Create `crates/vector/ops/input/classify.rs`:

```rust
/// Classify a file path into a type category for GitHub metadata.
///
/// Returns one of: `"test"`, `"config"`, `"doc"`, `"source"`.
pub fn classify_file_type(path: &str) -> &'static str {
    todo!()
}

/// Map a file extension to a human-readable language name.
///
/// Returns the extension as-is for unmapped extensions.
pub fn language_name(ext: &str) -> &str {
    todo!()
}

/// Check whether a file path looks like a test file.
pub fn is_test_path(path: &str) -> bool {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── classify_file_type ──

    #[test]
    fn test_classify_source_rs() {
        assert_eq!(classify_file_type("src/lib.rs"), "source");
    }

    #[test]
    fn test_classify_test_dir() {
        assert_eq!(classify_file_type("tests/unit/foo.rs"), "test");
    }

    #[test]
    fn test_classify_test_suffix_rs() {
        assert_eq!(classify_file_type("src/foo_test.rs"), "test");
    }

    #[test]
    fn test_classify_test_suffix_go() {
        assert_eq!(classify_file_type("pkg/handler_test.go"), "test");
    }

    #[test]
    fn test_classify_test_prefix_py() {
        assert_eq!(classify_file_type("tests/test_models.py"), "test");
    }

    #[test]
    fn test_classify_jest_test() {
        assert_eq!(classify_file_type("src/utils.test.ts"), "test");
    }

    #[test]
    fn test_classify_jest_spec() {
        assert_eq!(classify_file_type("src/utils.spec.tsx"), "test");
    }

    #[test]
    fn test_classify_dunder_tests() {
        assert_eq!(classify_file_type("__tests__/Button.tsx"), "test");
    }

    #[test]
    fn test_classify_config_toml() {
        assert_eq!(classify_file_type("Cargo.toml"), "config");
    }

    #[test]
    fn test_classify_config_yaml() {
        assert_eq!(classify_file_type("docker-compose.yaml"), "config");
    }

    #[test]
    fn test_classify_config_json() {
        assert_eq!(classify_file_type("package.json"), "config");
    }

    #[test]
    fn test_classify_doc_md() {
        assert_eq!(classify_file_type("docs/README.md"), "doc");
    }

    #[test]
    fn test_classify_doc_rst() {
        assert_eq!(classify_file_type("docs/intro.rst"), "doc");
    }

    #[test]
    fn test_classify_source_py() {
        assert_eq!(classify_file_type("src/main.py"), "source");
    }

    #[test]
    fn test_classify_source_no_ext() {
        assert_eq!(classify_file_type("Makefile"), "source");
    }

    // ── language_name ──

    #[test]
    fn test_language_rust() {
        assert_eq!(language_name("rs"), "rust");
    }

    #[test]
    fn test_language_python() {
        assert_eq!(language_name("py"), "python");
    }

    #[test]
    fn test_language_js() {
        assert_eq!(language_name("js"), "javascript");
    }

    #[test]
    fn test_language_ts() {
        assert_eq!(language_name("ts"), "typescript");
    }

    #[test]
    fn test_language_tsx() {
        assert_eq!(language_name("tsx"), "typescript");
    }

    #[test]
    fn test_language_go() {
        assert_eq!(language_name("go"), "go");
    }

    #[test]
    fn test_language_shell() {
        assert_eq!(language_name("sh"), "shell");
    }

    #[test]
    fn test_language_toml() {
        assert_eq!(language_name("toml"), "toml");
    }

    #[test]
    fn test_language_unknown() {
        assert_eq!(language_name("xyz"), "xyz");
    }

    // ── is_test_path ──

    #[test]
    fn test_is_test_path_tests_dir() {
        assert!(is_test_path("tests/unit/foo.rs"));
    }

    #[test]
    fn test_is_test_path_dunder() {
        assert!(is_test_path("__tests__/Button.tsx"));
    }

    #[test]
    fn test_is_test_path_suffix() {
        assert!(is_test_path("src/foo_test.go"));
    }

    #[test]
    fn test_is_test_path_spec() {
        assert!(is_test_path("src/utils.spec.ts"));
    }

    #[test]
    fn test_is_test_path_prefix_py() {
        assert!(is_test_path("tests/test_main.py"));
    }

    #[test]
    fn test_is_test_path_source() {
        assert!(!is_test_path("src/main.rs"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p axon classify -- --lib 2>&1 | tail -5`
Expected: FAIL — `todo!()` panics

- [ ] **Step 3: Implement the functions**

Replace the `todo!()` bodies in `classify.rs`:

```rust
/// Classify a file path into a type category for GitHub metadata.
///
/// Returns one of: `"test"`, `"config"`, `"doc"`, `"source"`.
pub fn classify_file_type(path: &str) -> &'static str {
    if is_test_path(path) {
        return "test";
    }
    let ext = path_extension(path);
    match ext {
        "toml" | "yaml" | "yml" | "json" => "config",
        "md" | "mdx" | "rst" | "txt" => "doc",
        _ => "source",
    }
}

/// Map a file extension to a human-readable language name.
///
/// Returns the extension as-is for unmapped extensions.
pub fn language_name(ext: &str) -> &str {
    match ext {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "go" => "go",
        "sh" | "bash" => "shell",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "md" | "mdx" => "markdown",
        other => other,
    }
}

/// Check whether a file path looks like a test file.
pub fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    // Directory-based: tests/, test/, __tests__/
    if lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains("/__tests__/")
        || lower.starts_with("tests/")
        || lower.starts_with("test/")
        || lower.starts_with("__tests__/")
    {
        return true;
    }
    let filename = path.rsplit('/').next().unwrap_or(path);
    let lower_fn = filename.to_ascii_lowercase();
    // Suffix: _test.rs, _test.go
    if lower_fn.contains("_test.") {
        return true;
    }
    // Prefix: test_*.py
    if lower_fn.starts_with("test_") {
        return true;
    }
    // JS/TS: .test.ts, .spec.ts, .test.tsx, .spec.tsx, .test.js, .spec.js
    if lower_fn.contains(".test.") || lower_fn.contains(".spec.") {
        return true;
    }
    false
}

/// Extract the file extension from a path (lowercase, no dot).
fn path_extension(path: &str) -> &str {
    let filename = path.rsplit('/').next().unwrap_or(path);
    match filename.rsplit_once('.') {
        Some((_, ext)) => ext,
        None => "",
    }
}
```

- [ ] **Step 4: Add module declaration to input.rs**

Add to the top of `crates/vector/ops/input.rs`:

```rust
pub mod classify;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p axon classify -- --lib 2>&1 | tail -5`
Expected: all 27 tests PASS

- [ ] **Step 6: Run full project check**

Run: `cargo check 2>&1 | tail -3`
Expected: clean compilation

- [ ] **Step 7: Commit**

```bash
git add crates/vector/ops/input.rs crates/vector/ops/input/classify.rs
git commit -m "$(cat <<'EOF'
feat(vector): add file classification heuristics for GitHub metadata

classify_file_type() → test/config/doc/source
language_name() → human-readable language from extension
is_test_path() → detects test dirs, suffixes, prefixes, .spec/.test
EOF
)"
```

---

### Task 3: Create Code Chunker Module

**Files:**
- Create: `crates/vector/ops/input/code.rs`
- Modify: `crates/vector/ops/input.rs` (add `pub mod code;`)

- [ ] **Step 1: Write failing tests**

Create `crates/vector/ops/input/code.rs`:

```rust
use text_splitter::{CodeSplitter, ChunkConfig};
use tree_sitter_language::LanguageFn;

/// Returns a tree-sitter language function for a given file extension.
/// Returns `None` for unsupported extensions (caller should fall back to `chunk_text()`).
fn language_for_extension(ext: &str) -> Option<LanguageFn> {
    todo!()
}

/// Chunk source code using tree-sitter AST-aware boundaries.
///
/// Returns `Some(chunks)` if a grammar exists for the extension,
/// `None` otherwise (caller falls back to `chunk_text()`).
/// Empty chunks are filtered out.
pub fn chunk_code(content: &str, file_extension: &str) -> Option<Vec<String>> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── language_for_extension ──

    #[test]
    fn ext_rs() {
        assert!(language_for_extension("rs").is_some());
    }

    #[test]
    fn ext_py() {
        assert!(language_for_extension("py").is_some());
    }

    #[test]
    fn ext_js() {
        assert!(language_for_extension("js").is_some());
    }

    #[test]
    fn ext_jsx() {
        assert!(language_for_extension("jsx").is_some());
    }

    #[test]
    fn ext_ts() {
        assert!(language_for_extension("ts").is_some());
    }

    #[test]
    fn ext_tsx() {
        assert!(language_for_extension("tsx").is_some());
    }

    #[test]
    fn ext_go() {
        assert!(language_for_extension("go").is_some());
    }

    #[test]
    fn ext_sh() {
        assert!(language_for_extension("sh").is_some());
    }

    #[test]
    fn ext_bash() {
        assert!(language_for_extension("bash").is_some());
    }

    #[test]
    fn ext_unknown() {
        assert!(language_for_extension("yaml").is_none());
    }

    #[test]
    fn ext_empty() {
        assert!(language_for_extension("").is_none());
    }

    // ── chunk_code ──

    #[test]
    fn unsupported_ext_returns_none() {
        assert!(chunk_code("key: value", "yaml").is_none());
    }

    #[test]
    fn empty_content_returns_empty() {
        let chunks = chunk_code("", "rs").unwrap();
        assert!(chunks.is_empty());
    }

    #[test]
    fn small_rust_file_single_chunk() {
        let code = r#"
fn hello() {
    println!("hello");
}
"#;
        let chunks = chunk_code(code, "rs").unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].contains("fn hello"));
    }

    #[test]
    fn multi_function_rust_file() {
        // Two small functions — each should fit in one chunk
        let code = r#"
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn subtract(a: i32, b: i32) -> i32 {
    a - b
}
"#;
        let chunks = chunk_code(code, "rs").unwrap();
        // Both fit in one chunk (well under 500 chars min), so they stay together
        assert!(!chunks.is_empty());
        // Content coverage: all functions appear across chunks
        let joined = chunks.join("");
        assert!(joined.contains("fn add"));
        assert!(joined.contains("fn subtract"));
    }

    #[test]
    fn large_function_gets_split() {
        // Generate a function body > 2000 chars to force splitting
        let mut body = String::from("fn big() {\n");
        for i in 0..150 {
            body.push_str(&format!("    let x_{i} = {i};\n"));
        }
        body.push_str("}\n");
        assert!(body.len() > 2000, "test setup: body should exceed max chunk size");
        let chunks = chunk_code(&body, "rs").unwrap();
        assert!(chunks.len() > 1, "large function should produce multiple chunks");
    }

    #[test]
    fn python_code_chunking() {
        let code = r#"
def hello():
    print("hello")

def world():
    print("world")
"#;
        let chunks = chunk_code(code, "py").unwrap();
        assert!(!chunks.is_empty());
        let joined = chunks.join("");
        assert!(joined.contains("def hello"));
        assert!(joined.contains("def world"));
    }

    #[test]
    fn typescript_code_chunking() {
        let code = r#"
function greet(name: string): string {
    return `Hello, ${name}`;
}

export const PI = 3.14159;
"#;
        let chunks = chunk_code(code, "ts").unwrap();
        assert!(!chunks.is_empty());
        let joined = chunks.join("");
        assert!(joined.contains("function greet"));
    }

    #[test]
    fn no_empty_chunks_in_output() {
        let code = "fn f() {}\n\n\n\n\nfn g() {}";
        let chunks = chunk_code(code, "rs").unwrap();
        for (i, c) in chunks.iter().enumerate() {
            assert!(!c.trim().is_empty(), "chunk {i} should not be empty");
        }
    }
}
```

- [ ] **Step 2: Add module declaration to input.rs**

Add to `crates/vector/ops/input.rs` (below the `pub mod classify;` added in Task 2):

```rust
pub mod code;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p axon input::code -- --lib 2>&1 | tail -5`
Expected: FAIL — `todo!()` panics

- [ ] **Step 4: Implement the functions**

Replace the `todo!()` bodies in `code.rs`:

```rust
use text_splitter::{ChunkConfig, CodeSplitter};
use tree_sitter_language::LanguageFn;

/// Returns a tree-sitter language function for a given file extension.
/// Returns `None` for unsupported extensions (caller should fall back to `chunk_text()`).
fn language_for_extension(ext: &str) -> Option<LanguageFn> {
    match ext {
        "rs" => Some(tree_sitter_rust::LANGUAGE),
        "py" => Some(tree_sitter_python::LANGUAGE),
        "js" | "jsx" => Some(tree_sitter_javascript::LANGUAGE),
        "ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX),
        "go" => Some(tree_sitter_go::LANGUAGE),
        "sh" | "bash" => Some(tree_sitter_bash::LANGUAGE),
        _ => None,
    }
}

/// Chunk source code using tree-sitter AST-aware boundaries.
///
/// Returns `Some(chunks)` if a grammar exists for the extension,
/// `None` otherwise (caller falls back to `chunk_text()`).
/// Empty chunks are filtered out.
pub fn chunk_code(content: &str, file_extension: &str) -> Option<Vec<String>> {
    let lang = language_for_extension(file_extension)?;
    let config = ChunkConfig::new(500..2000);
    let splitter = CodeSplitter::new(lang, config).expect("valid language");
    let chunks: Vec<String> = splitter
        .chunks(content)
        .map(|c| c.to_string())
        .filter(|c| !c.trim().is_empty())
        .collect();
    Some(chunks)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p axon input::code -- --lib 2>&1 | tail -5`
Expected: all 18 tests PASS

- [ ] **Step 6: Run full test suite**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: no regressions (existing `chunk_text` tests still pass)

- [ ] **Step 7: Commit**

```bash
git add crates/vector/ops/input.rs crates/vector/ops/input/code.rs
git commit -m "$(cat <<'EOF'
feat(vector): add AST-aware code chunking via tree-sitter

chunk_code() splits source files at function/struct/class boundaries
using text-splitter + tree-sitter grammars. Returns None for unknown
extensions (caller falls back to chunk_text).

Supported: .rs, .py, .js, .jsx, .ts, .tsx, .go, .sh, .bash
Chunk range: 500-2000 chars (prefers largest AST node that fits)
EOF
)"
```

---

## Chunk 2: Embedding Pipeline + Unified Payload

### Task 4: Add `embed_code_with_metadata()` to TEI Pipeline

**Files:**
- Modify: `crates/vector/ops/tei.rs`

- [ ] **Step 1: Add the new embedding function**

Add after `embed_text_with_extra_payload()` (after line 143 in `tei.rs`):

```rust
/// Embed source code content using AST-aware chunking when a grammar is available.
///
/// Tries `chunk_code(content, file_extension)` first. If no grammar exists for
/// the extension, falls back to `chunk_text(content)`. The rest of the pipeline
/// (TEI embed, Qdrant upsert, stale tail cleanup) is unchanged.
pub async fn embed_code_with_metadata(
    cfg: &Config,
    content: &str,
    url: &str,
    source_type: &str,
    title: Option<&str>,
    file_extension: &str,
    extra: Option<&serde_json::Value>,
) -> Result<usize, Box<dyn Error>> {
    if content.trim().is_empty() {
        return Ok(0);
    }
    let chunks = match input::code::chunk_code(content, file_extension) {
        Some(c) if !c.is_empty() => c,
        _ => input::chunk_text(content),
    };
    if chunks.is_empty() {
        return Ok(0);
    }
    embed_chunks_impl(cfg, chunks, url, source_type, title, extra).await
}
```

- [ ] **Step 2: Extract shared chunk→embed pipeline**

The existing `embed_text_impl` does chunk + embed. We need to split it so `embed_code_with_metadata` can supply pre-chunked content. Refactor `embed_text_impl` to call a new `embed_chunks_impl`:

In `tei.rs`, rename the body of `embed_text_impl` from line 47 (`let chunks = ...`) onward into a new private function:

```rust
/// Shared implementation: takes pre-chunked text, embeds via TEI, upserts to Qdrant.
async fn embed_chunks_impl(
    cfg: &Config,
    chunks: Vec<String>,
    url: &str,
    source_type: &str,
    title: Option<&str>,
    extra: Option<&serde_json::Value>,
) -> Result<usize, Box<dyn Error>> {
    let vectors = tei_embed(cfg, &chunks).await?;
    if vectors.is_empty() {
        return Err(format!("TEI returned no vectors for {url}").into());
    }
    if vectors.len() != chunks.len() {
        return Err(format!(
            "TEI vector count mismatch for {url}: {} vectors for {} chunks",
            vectors.len(),
            chunks.len()
        )
        .into());
    }
    let dim = vectors[0].len();
    if qdrant_store::collection_needs_init(&cfg.collection) {
        qdrant_store::ensure_collection(cfg, dim).await?;
    }
    let domain = spider::url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    let timestamp = chrono::Utc::now().to_rfc3339();
    let mut points = Vec::with_capacity(vectors.len());
    for (idx, (chunk, vecv)) in chunks.into_iter().zip(vectors.into_iter()).enumerate() {
        let point_id = uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_URL,
            format!("{url}:{idx}").as_bytes(),
        );
        let mut payload = serde_json::json!({
            "url": url,
            "domain": domain,
            "source_type": source_type,
            "source_command": source_type,
            "content_type": "text",
            "chunk_index": idx,
            "chunk_text": chunk,
            "scraped_at": timestamp,
        });
        if let Some(t) = title {
            payload["title"] = serde_json::Value::String(t.to_string());
        }
        if let Some(serde_json::Value::Object(map)) = extra {
            for (k, v) in map {
                payload[k] = v.clone();
            }
        }
        points.push(serde_json::json!({
            "id": point_id.to_string(),
            "vector": vecv,
            "payload": payload,
        }));
    }
    let new_count = points.len();
    qdrant_store::qdrant_upsert(cfg, &points).await?;
    qdrant_delete_stale_tail(cfg, url, new_count).await?;
    Ok(new_count)
}
```

Then simplify `embed_text_impl`:

```rust
async fn embed_text_impl(
    cfg: &Config,
    content: &str,
    url: &str,
    source_type: &str,
    title: Option<&str>,
    extra: Option<&serde_json::Value>,
) -> Result<usize, Box<dyn Error>> {
    if content.trim().is_empty() {
        return Ok(0);
    }
    let chunks = input::chunk_text(content);
    if chunks.is_empty() {
        return Ok(0);
    }
    embed_chunks_impl(cfg, chunks, url, source_type, title, extra).await
}
```

- [ ] **Step 3: Run full test suite to verify no regressions**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: all existing tests pass — the refactor is behavior-preserving

- [ ] **Step 4: Commit**

```bash
git add crates/vector/ops/tei.rs
git commit -m "$(cat <<'EOF'
feat(vector): add embed_code_with_metadata with AST chunking fallback

Extract embed_chunks_impl from embed_text_impl to share the TEI→Qdrant
pipeline. New embed_code_with_metadata tries chunk_code() first, falls
back to chunk_text() for unsupported extensions.
EOF
)"
```

---

### Task 5: Unified GitHub Payload Builder

**Files:**
- Modify: `crates/ingest/github/meta.rs`

- [ ] **Step 1: Write failing tests for the unified builder**

Add at the end of `meta.rs`:

```rust
/// Parameters for the unified GitHub payload builder.
///
/// All fields are `Option` except the required common ones.
/// Non-applicable fields are serialized as `null` so every chunk
/// has the same 29 `gh_*` key set.
pub struct GitHubPayloadParams {
    // ── Common (all chunk types) ──
    pub repo: String,
    pub owner: String,
    pub branch: Option<String>,
    pub default_branch: Option<String>,
    pub content_kind: String,
    pub repo_description: Option<String>,
    pub pushed_at: Option<String>,
    pub is_private: Option<bool>,

    // ── Repo metadata ──
    pub stars: Option<u32>,
    pub forks: Option<u32>,
    pub open_issues: Option<u32>,
    pub language: Option<String>,
    pub topics: Option<Vec<String>>,
    pub is_fork: Option<bool>,
    pub is_archived: Option<bool>,

    // ── Issue / PR ──
    pub issue_number: Option<u64>,
    pub state: Option<String>,
    pub author: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub comment_count: Option<u32>,
    pub labels: Option<Vec<String>>,
    pub is_pr: Option<bool>,
    pub merged_at: Option<String>,
    pub is_draft: Option<bool>,

    // ── File ──
    pub file_path: Option<String>,
    pub file_language: Option<String>,
    pub file_type: Option<String>,
    pub is_test: Option<bool>,
    pub file_size_bytes: Option<u64>,
    pub chunking_method: Option<String>,
}

/// Build the unified `gh_*` payload for any GitHub chunk type.
///
/// Non-applicable `Option` fields serialize as JSON `null`. Every chunk
/// carries all 31 keys regardless of `content_kind`.
pub fn build_github_payload(params: &GitHubPayloadParams) -> Value {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_params() -> GitHubPayloadParams {
        GitHubPayloadParams {
            repo: "owner/repo".to_string(),
            owner: "owner".to_string(),
            branch: Some("main".to_string()),
            default_branch: Some("main".to_string()),
            content_kind: "file".to_string(),
            repo_description: Some("A test repo".to_string()),
            pushed_at: Some("2026-03-10T00:00:00Z".to_string()),
            is_private: Some(false),
            stars: None,
            forks: None,
            open_issues: None,
            language: None,
            topics: None,
            is_fork: None,
            is_archived: None,
            issue_number: None,
            state: None,
            author: None,
            created_at: None,
            updated_at: None,
            comment_count: None,
            labels: None,
            is_pr: None,
            merged_at: None,
            is_draft: None,
            file_path: Some("src/lib.rs".to_string()),
            file_language: Some("rust".to_string()),
            file_type: Some("source".to_string()),
            is_test: Some(false),
            file_size_bytes: Some(4280),
            chunking_method: Some("tree-sitter".to_string()),
        }
    }

    #[test]
    fn payload_has_all_29_keys() {
        let p = build_github_payload(&base_params());
        let obj = p.as_object().expect("payload must be an object");
        assert_eq!(obj.len(), 31, "payload must have exactly 31 gh_* keys");
    }

    #[test]
    fn common_fields_always_present() {
        let p = build_github_payload(&base_params());
        assert_eq!(p["gh_repo"], "owner/repo");
        assert_eq!(p["gh_owner"], "owner");
        assert_eq!(p["gh_content_kind"], "file");
        assert_eq!(p["gh_default_branch"], "main");
    }

    #[test]
    fn file_fields_populated() {
        let p = build_github_payload(&base_params());
        assert_eq!(p["gh_file_path"], "src/lib.rs");
        assert_eq!(p["gh_file_language"], "rust");
        assert_eq!(p["gh_file_type"], "source");
        assert_eq!(p["gh_is_test"], false);
        assert_eq!(p["gh_chunking_method"], "tree-sitter");
    }

    #[test]
    fn issue_fields_null_for_file_chunks() {
        let p = build_github_payload(&base_params());
        assert!(p["gh_issue_number"].is_null());
        assert!(p["gh_state"].is_null());
        assert!(p["gh_is_pr"].is_null());
    }

    #[test]
    fn repo_metadata_fields_null_for_file_chunks() {
        let p = build_github_payload(&base_params());
        assert!(p["gh_stars"].is_null());
        assert!(p["gh_forks"].is_null());
        assert!(p["gh_topics"].is_null());
    }

    #[test]
    fn issue_params_produce_correct_payload() {
        let mut params = base_params();
        params.content_kind = "issue".to_string();
        params.issue_number = Some(42);
        params.state = Some("open".to_string());
        params.author = Some("alice".to_string());
        params.is_pr = Some(false);
        params.file_path = None;
        params.file_language = None;
        params.file_type = None;
        params.is_test = None;
        params.file_size_bytes = None;
        params.chunking_method = None;
        let p = build_github_payload(&params);
        assert_eq!(p["gh_issue_number"], 42);
        assert_eq!(p["gh_state"], "open");
        assert_eq!(p["gh_is_pr"], false);
        assert!(p["gh_file_path"].is_null());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p axon meta::tests -- --lib 2>&1 | tail -5`
Expected: FAIL — `todo!()` panic

- [ ] **Step 3: Implement `build_github_payload()`**

Replace the `todo!()`:

```rust
pub fn build_github_payload(params: &GitHubPayloadParams) -> Value {
    json!({
        "gh_repo": params.repo,
        "gh_owner": params.owner,
        "gh_branch": params.branch,
        "gh_default_branch": params.default_branch,
        "gh_content_kind": params.content_kind,
        "gh_repo_description": params.repo_description,
        "gh_pushed_at": params.pushed_at,
        "gh_is_private": params.is_private,
        "gh_stars": params.stars,
        "gh_forks": params.forks,
        "gh_open_issues": params.open_issues,
        "gh_language": params.language,
        "gh_topics": params.topics,
        "gh_is_fork": params.is_fork,
        "gh_is_archived": params.is_archived,
        "gh_issue_number": params.issue_number,
        "gh_state": params.state,
        "gh_author": params.author,
        "gh_created_at": params.created_at,
        "gh_updated_at": params.updated_at,
        "gh_comment_count": params.comment_count,
        "gh_labels": params.labels,
        "gh_is_pr": params.is_pr,
        "gh_merged_at": params.merged_at,
        "gh_is_draft": params.is_draft,
        "gh_file_path": params.file_path,
        "gh_file_language": params.file_language,
        "gh_file_type": params.file_type,
        "gh_is_test": params.is_test,
        "gh_file_size_bytes": params.file_size_bytes,
        "gh_chunking_method": params.chunking_method,
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p axon meta::tests -- --lib 2>&1 | tail -5`
Expected: all 6 tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ingest/github/meta.rs
git commit -m "$(cat <<'EOF'
feat(ingest): unified GitHubPayloadParams + build_github_payload

Replaces 3 per-type payload builders with a single unified builder.
All 31 gh_* fields present on every chunk; non-applicable fields are null.
Old builders preserved temporarily — removed when callers migrate.
EOF
)"
```

---

## Chunk 3: GitHub Integration (Files, Issues, Wiki, Orchestration)

### Task 6: Introduce Common Fields Struct + Update Orchestration

**Files:**
- Modify: `crates/ingest/github.rs`

- [ ] **Step 1: Define `GitHubCommonFields` struct**

Add after imports in `github.rs`:

```rust
/// Common fields extracted from `repos().get()` and passed to all sub-tasks.
/// Every GitHub chunk (file, issue, PR, wiki, repo_metadata) gets these.
pub(crate) struct GitHubCommonFields {
    pub owner: String,
    pub name: String,
    pub repo_slug: String,           // "owner/name"
    pub default_branch: String,
    pub repo_description: Option<String>,
    pub pushed_at: Option<String>,   // RFC3339
    pub is_private: Option<bool>,
}
```

- [ ] **Step 2: Extract common fields from repo_info in `ingest_github()`**

Modify `ingest_github()` to build `GitHubCommonFields` from the `repo_info` response:

```rust
let common = GitHubCommonFields {
    owner: owner.clone(),
    name: name.clone(),
    repo_slug: format!("{owner}/{name}"),
    default_branch: repo_info
        .default_branch
        .as_deref()
        .unwrap_or("main")
        .to_string(),
    repo_description: repo_info.description.clone(),
    pushed_at: repo_info.pushed_at.map(|dt| dt.to_rfc3339()),
    is_private: repo_info.private,
};
```

- [ ] **Step 3: Pass `&common` to all sub-tasks**

Update the `tokio::join!` call signatures. Each sub-function gains a `common: &GitHubCommonFields` parameter. For now, the sub-functions ignore it (wired in Tasks 7-9). Example:

```rust
let (files_result, metadata_result, issues_result, prs_result, wiki_result) = tokio::join!(
    files::embed_files(cfg, &common, include_source, token),
    embed_repo_metadata(cfg, &common, &repo_info),
    issues::ingest_issues(cfg, &common, &octo),
    issues::ingest_pull_requests(cfg, &common, &octo),
    wiki::ingest_wiki(cfg, &common, token),
);
```

- [ ] **Step 4: Update function signatures (compile-fix pass)**

Update `embed_repo_metadata` to accept `&GitHubCommonFields`:

```rust
async fn embed_repo_metadata(
    cfg: &Config,
    common: &GitHubCommonFields,
    repo_info: &models::Repository,
) -> Result<usize, Box<dyn Error>>
```

Inside, switch from `build_github_repo_extra_payload` to `build_github_payload` with repo metadata fields:

```rust
use super::github::meta::{GitHubPayloadParams, build_github_payload};

let extra = build_github_payload(&GitHubPayloadParams {
    repo: common.repo_slug.clone(),
    owner: common.owner.clone(),
    branch: None,
    default_branch: Some(common.default_branch.clone()),
    content_kind: "repo_metadata".to_string(),
    repo_description: common.repo_description.clone(),
    pushed_at: common.pushed_at.clone(),
    is_private: common.is_private,
    stars: repo_info.stargazers_count,
    forks: repo_info.forks_count,
    open_issues: repo_info.open_issues_count,
    language: repo_info.language.as_ref().and_then(|v| v.as_str()).map(String::from),
    topics: repo_info.topics.clone(),
    is_fork: repo_info.fork,
    is_archived: repo_info.archived,
    issue_number: None,
    state: None,
    author: None,
    created_at: repo_info.created_at.map(|dt| dt.to_rfc3339()),
    updated_at: None,
    comment_count: None,
    labels: None,
    is_pr: None,
    merged_at: None,
    is_draft: None,
    file_path: None,
    file_language: None,
    file_type: None,
    is_test: None,
    file_size_bytes: None,
    chunking_method: None,
});
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: clean compilation (sub-functions accept but don't yet use `common`)

- [ ] **Step 6: Commit**

```bash
git add crates/ingest/github.rs
git commit -m "$(cat <<'EOF'
refactor(ingest): extract GitHubCommonFields, pass to all sub-tasks

Common repo metadata (owner, name, description, pushed_at, is_private)
now flows from repos().get() to all 5 concurrent ingest pipelines.
embed_repo_metadata switched to unified build_github_payload.
EOF
)"
```

---

### Task 7: Update `files.rs` — Code-Aware Chunking + File Metadata

**Files:**
- Modify: `crates/ingest/github/files.rs`

- [ ] **Step 1: Update `embed_files` signature**

Change the signature to accept `&GitHubCommonFields` instead of individual `owner`/`name`/`default_branch`:

```rust
pub async fn embed_files(
    cfg: &Config,
    common: &super::GitHubCommonFields,
    include_source: bool,
    token: Option<&str>,
) -> Result<usize, Box<dyn Error>>
```

- [ ] **Step 2: Switch embedding call**

For each file, instead of:
```rust
embed_text_with_metadata(cfg, &text, &source_url, "github", Some(&path))
```

Use:
```rust
use crate::crates::vector::ops::input::classify::{classify_file_type, language_name, is_test_path};
use crate::crates::vector::ops::tei::embed_code_with_metadata;
use super::meta::{GitHubPayloadParams, build_github_payload};

let ext = path.rsplit('.').next().unwrap_or("");
let extra = build_github_payload(&GitHubPayloadParams {
    repo: common.repo_slug.clone(),
    owner: common.owner.clone(),
    branch: Some(common.default_branch.clone()),
    default_branch: Some(common.default_branch.clone()),
    content_kind: "file".to_string(),
    repo_description: common.repo_description.clone(),
    pushed_at: common.pushed_at.clone(),
    is_private: common.is_private,
    stars: None,
    forks: None,
    open_issues: None,
    language: None,
    topics: None,
    is_fork: None,
    is_archived: None,
    issue_number: None,
    state: None,
    author: None,
    created_at: None,
    updated_at: None,
    comment_count: None,
    labels: None,
    is_pr: None,
    merged_at: None,
    is_draft: None,
    file_path: Some(path.clone()),
    file_language: Some(language_name(ext).to_string()),
    file_type: Some(classify_file_type(&path).to_string()),
    is_test: Some(is_test_path(&path)),
    file_size_bytes: Some(text.len() as u64),
    chunking_method: None,  // Set after chunking decision
});

let chunks = embed_code_with_metadata(
    cfg,
    &text,
    &source_url,
    "github",
    Some(&path),
    ext,
    Some(&extra),
).await?;
```

**Note on `chunking_method`:** The spec says to tag chunks with `"tree-sitter"` or `"prose"`. Since `embed_code_with_metadata` makes that decision internally, we set it based on whether `chunk_code()` returns `Some`:

```rust
use crate::crates::vector::ops::input::code::chunk_code;

let chunking_method = if chunk_code("", ext).is_some() {
    // Grammar exists — will use tree-sitter (even if fallback triggers on empty)
    "tree-sitter"
} else {
    "prose"
};
// Set in payload before calling embed:
// chunking_method: Some(chunking_method.to_string()),
```

Actually, simpler: `language_for_extension` check. But since that's private, use the public `chunk_code` with a dummy check. Better approach — just check the extension:

```rust
let has_grammar = matches!(ext, "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "go" | "sh" | "bash");
let chunking_method = if has_grammar { "tree-sitter" } else { "prose" };
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: clean compilation

- [ ] **Step 4: Run existing tests**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/ingest/github/files.rs
git commit -m "$(cat <<'EOF'
feat(ingest): code-aware chunking + file metadata in GitHub file embeds

embed_files now uses embed_code_with_metadata for AST-aware splitting.
Each file chunk carries gh_file_path, gh_file_language, gh_file_type,
gh_is_test, gh_file_size_bytes, gh_chunking_method.
EOF
)"
```

---

### Task 8: Update `issues.rs` — Unified Payload

**Files:**
- Modify: `crates/ingest/github/issues.rs`

- [ ] **Step 1: Update function signatures**

```rust
pub async fn ingest_issues(
    cfg: &Config,
    common: &super::GitHubCommonFields,
    octo: &Octocrab,
) -> Result<usize, Box<dyn Error>>

pub async fn ingest_pull_requests(
    cfg: &Config,
    common: &super::GitHubCommonFields,
    octo: &Octocrab,
) -> Result<usize, Box<dyn Error>>
```

- [ ] **Step 2: Switch to `build_github_payload` in `ingest_issues`**

Replace `build_github_issue_extra_payload(issue)` with:

```rust
use super::meta::{GitHubPayloadParams, build_github_payload};

let extra = build_github_payload(&GitHubPayloadParams {
    repo: common.repo_slug.clone(),
    owner: common.owner.clone(),
    branch: None,
    default_branch: Some(common.default_branch.clone()),
    content_kind: "issue".to_string(),
    repo_description: common.repo_description.clone(),
    pushed_at: common.pushed_at.clone(),
    is_private: common.is_private,
    stars: None,
    forks: None,
    open_issues: None,
    language: None,
    topics: None,
    is_fork: None,
    is_archived: None,
    issue_number: Some(issue.number as u64),
    state: Some(issue_state_str(&issue.state).to_string()),
    author: Some(issue.user.login.clone()),
    created_at: Some(issue.created_at.to_rfc3339()),
    updated_at: Some(issue.updated_at.to_rfc3339()),
    comment_count: Some(issue.comments as u32),
    labels: Some(issue.labels.iter().map(|l| l.name.clone()).collect()),
    is_pr: Some(false),
    merged_at: None,
    is_draft: None,
    file_path: None,
    file_language: None,
    file_type: None,
    is_test: None,
    file_size_bytes: None,
    chunking_method: None,
});
```

- [ ] **Step 3: Switch to `build_github_payload` in `ingest_pull_requests`**

Same pattern, with `content_kind: "pr"`, `is_pr: Some(true)`, plus PR-specific fields (`merged_at`, `is_draft`).

- [ ] **Step 4: Import `issue_state_str` or move it**

`issue_state_str` currently lives in `meta.rs`. Make it `pub(crate)` so `issues.rs` can use it:

In `meta.rs`, change:
```rust
fn issue_state_str(state: &models::IssueState) -> &'static str {
```
to:
```rust
pub(crate) fn issue_state_str(state: &models::IssueState) -> &'static str {
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`

- [ ] **Step 6: Commit**

```bash
git add crates/ingest/github/issues.rs crates/ingest/github/meta.rs
git commit -m "$(cat <<'EOF'
refactor(ingest): issues + PRs use unified build_github_payload

Both ingest_issues and ingest_pull_requests now build payloads via
the unified builder with common fields from GitHubCommonFields.
EOF
)"
```

---

### Task 9: Update `wiki.rs` — Unified Payload

**Files:**
- Modify: `crates/ingest/github/wiki.rs`

- [ ] **Step 1: Update signature**

```rust
pub async fn ingest_wiki(
    cfg: &Config,
    common: &super::GitHubCommonFields,
    token: Option<&str>,
) -> Result<usize, Box<dyn Error>>
```

- [ ] **Step 2: Switch from `embed_text_with_metadata` to `embed_text_with_extra_payload`**

For each wiki file, build the payload:

```rust
use super::meta::{GitHubPayloadParams, build_github_payload};

let extra = build_github_payload(&GitHubPayloadParams {
    repo: common.repo_slug.clone(),
    owner: common.owner.clone(),
    branch: None,  // wiki has no branch concept
    default_branch: Some(common.default_branch.clone()),
    content_kind: "wiki".to_string(),
    repo_description: common.repo_description.clone(),
    pushed_at: common.pushed_at.clone(),
    is_private: common.is_private,
    // All other fields: None
    stars: None, forks: None, open_issues: None, language: None,
    topics: None, is_fork: None, is_archived: None,
    issue_number: None, state: None, author: None, created_at: None,
    updated_at: None, comment_count: None, labels: None, is_pr: None,
    merged_at: None, is_draft: None,
    file_path: None, file_language: None, file_type: None, is_test: None,
    file_size_bytes: None, chunking_method: None,
});

embed_text_with_extra_payload(cfg, &text, &source_url, "github", Some(&filename), &extra).await?
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`

- [ ] **Step 4: Run full test suite**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/ingest/github/wiki.rs
git commit -m "$(cat <<'EOF'
refactor(ingest): wiki uses unified build_github_payload

Wiki chunks now carry common repo metadata (owner, description,
pushed_at, is_private) via the unified payload builder.
EOF
)"
```

---

### Task 10: Remove Old Payload Builders

**Files:**
- Modify: `crates/ingest/github/meta.rs`

- [ ] **Step 1: Delete the three old builder functions**

Remove `build_github_repo_extra_payload`, `build_github_issue_extra_payload`, and `build_github_pr_extra_payload`. They are now dead code — all callers use `build_github_payload`.

- [ ] **Step 2: Verify no remaining references**

Run: `cargo check 2>&1 | tail -5`
Expected: clean — if there are errors, some caller was missed.

Also grep to confirm:

Run: `grep -rn 'build_github_repo_extra_payload\|build_github_issue_extra_payload\|build_github_pr_extra_payload' crates/`
Expected: no matches

- [ ] **Step 3: Commit**

```bash
git add crates/ingest/github/meta.rs
git commit -m "$(cat <<'EOF'
cleanup(ingest): remove per-type payload builders

build_github_repo_extra_payload, build_github_issue_extra_payload,
build_github_pr_extra_payload replaced by unified build_github_payload.
EOF
)"
```

---

## Chunk 4: CLI Default Flip + Refresh Scheduling

### Task 11: Flip `--include-source` Default + Add `--no-source`

**Files:**
- Modify: `crates/core/config/cli.rs`
- Modify: `crates/core/config/types/subconfigs.rs`
- Modify: `crates/core/config/types/config_impls.rs`
- Modify: `crates/core/config/parse/build_config.rs`

- [ ] **Step 1: Change default to `true` in subconfigs.rs**

In `crates/core/config/types/subconfigs.rs`, change line 77:
```rust
github_include_source: false,
```
to:
```rust
github_include_source: true,
```

- [ ] **Step 2: Change default to `true` in config_impls.rs**

In `crates/core/config/types/config_impls.rs`, change line 71:
```rust
github_include_source: false,
```
to:
```rust
github_include_source: true,
```

- [ ] **Step 3: Add `--no-source` flag to CLI args**

In `crates/core/config/cli.rs`, add after the `include_source` field:

```rust
/// Skip source code files when ingesting a GitHub repository.
#[arg(long = "no-source")]
pub(super) no_source: bool,
```

- [ ] **Step 4: Wire `--no-source` in build_config.rs**

In `crates/core/config/parse/build_config.rs`:

**First**, change the initial value at line 19 from:
```rust
let mut github_include_source = false;
```
to:
```rust
let mut github_include_source = true;
```

**Then**, where `github_include_source` is set (around line 130), replace the old `github_include_source = args.include_source;` with:
```rust
// --no-source overrides the default (true). --include-source is now a no-op.
if args.no_source {
    github_include_source = false;
}
```

**Critical:** Both changes are required. The initial value is what gets used at runtime — changing only the `Default` trait impls is insufficient.

- [ ] **Step 5: Update test assertions in subconfigs.rs**

In `crates/core/config/types/subconfigs.rs` (around line 301), update:
```rust
assert!(!c.github_include_source);
```
to:
```rust
assert!(c.github_include_source);
```

- [ ] **Step 6: Check for ALL `github_include_source` references**

Run: `grep -rn 'github_include_source' crates/ --include='*.rs' | grep -v 'target/'`

Known locations that need updating:
- `crates/core/config/types/subconfigs.rs:301` — test assertion (already covered in Step 5)
- `crates/ingest/classify.rs:157` — `github_include_source_propagated()` test. This test passes `true` to `classify_target`, so it should still pass. **Verify it does not assert on the old default.**
- Any inline `Config { ... }` literals in `research.rs`, `search.rs`, `crates/jobs/common/` — change `github_include_source: false` to `github_include_source: true`.

- [ ] **Step 7: Verify it compiles and tests pass**

Run: `cargo check && cargo test --lib 2>&1 | tail -5`

- [ ] **Step 8: Commit**

```bash
git add crates/core/config/
git commit -m "$(cat <<'EOF'
feat(cli): source code included by default in GitHub ingest

github_include_source default flipped false→true.
New --no-source flag to opt out. --include-source is now a no-op.
EOF
)"
```

---

### Task 12: Extend Refresh Schedule for GitHub Repos

**Files:**
- Modify: `crates/jobs/refresh.rs` (schema + struct)
- Modify: `crates/jobs/refresh/schedule.rs` (create + claim logic)

- [ ] **Step 1: Add columns to schema**

In `crates/jobs/refresh.rs`, in the `ensure_schema` function, add after the `CREATE TABLE`:

```sql
ALTER TABLE axon_refresh_schedules ADD COLUMN IF NOT EXISTS source_type TEXT;
ALTER TABLE axon_refresh_schedules ADD COLUMN IF NOT EXISTS target TEXT;
```

These are nullable — existing URL-based schedules have `NULL` for both.

- [ ] **Step 2: Add fields to `RefreshSchedule` struct**

In `crates/jobs/refresh.rs`, add to the `RefreshSchedule` struct:

```rust
pub source_type: Option<String>,  // None = URL refresh, Some("github") = GitHub repo
pub target: Option<String>,       // "owner/repo" for GitHub schedules
```

- [ ] **Step 3: Add fields to `RefreshScheduleCreate`**

In `crates/jobs/refresh/schedule.rs`, add to `RefreshScheduleCreate`:

```rust
pub source_type: Option<String>,
pub target: Option<String>,
```

- [ ] **Step 4: Update ALL SQL queries that SELECT into `RefreshSchedule`**

**Critical:** `RefreshSchedule` derives `FromRow`. Every SQL query that returns `RefreshSchedule` rows must include the new `source_type` and `target` columns, or `sqlx` will panic at runtime.

Update these queries:
1. `create_refresh_schedule_with_pool` in `schedule.rs` (~line 85): Add `source_type, target` to both the INSERT column list AND the RETURNING clause. Bind the new fields.
2. `list_refresh_schedules_with_pool` in `schedule.rs` (~line 119): Add `source_type, target` to the SELECT column list (or use `SELECT *` if the query already does).
3. `claim_due_refresh_schedules_with_pool` in `schedule.rs` (~line 190): Add `source_type, target` to the CTE's SELECT and the final RETURNING clause.

If any of these queries use `SELECT *`, they're fine — the new columns will be picked up automatically. But if they use explicit column lists, they **must** be updated.

- [ ] **Step 5: Update claim logic for GitHub schedules**

In `crates/cli/commands/refresh/schedule.rs`, function `run_refresh_schedule_due_sweep` (~line 374), in the `for schedule in &claimed` loop (~line 396), add a branch **before** the existing `resolve_schedule_urls` call:

```rust
// Inside the `for schedule in &claimed` loop, BEFORE resolve_schedule_urls:
if schedule.source_type.as_deref() == Some("github") {
    if let Some(target) = &schedule.target {
        // Lightweight GitHub API check
        match check_github_pushed_at(cfg, target).await {
            Ok(pushed_at) => {
                if should_reingest_github(&pushed_at, schedule.last_run_at) {
                    // Enqueue a full re-ingest via the existing ingest job system
                    use crate::crates::jobs::ingest::types::IngestSource;
                    use crate::crates::jobs::ingest::ops::start_ingest_job;
                    match start_ingest_job(cfg, IngestSource::Github {
                        repo: target.clone(),
                        include_source: true,
                    }).await {
                        Ok(job_id) => {
                            dispatched += 1;
                            jobs.push(serde_json::json!({"schedule": schedule.name, "job_id": job_id}));
                        }
                        Err(err) => {
                            log_warn(&format!("refresh github ingest failed for {target}: {err}"));
                            failed += 1;
                        }
                    }
                } else {
                    skipped += 1;
                }
            }
            Err(err) => {
                log_warn(&format!("refresh github API check failed for {target}: {err}"));
                failed += 1;
            }
        }
        // Update last_run_at regardless
        let next = now + Duration::seconds(schedule.every_seconds);
        let _ = mark_refresh_schedule_ran_with_pool(&pool, schedule.id, next).await;
        continue;  // Skip the URL-based refresh path below
    }
}
// Existing URL refresh logic follows (unchanged)...
```

**Key functions to implement:**

```rust
/// Check the GitHub API for the repo's pushed_at timestamp.
async fn check_github_pushed_at(cfg: &Config, repo: &str) -> Result<String, Box<dyn Error>> {
    let url = format!("https://api.github.com/repos/{repo}");
    let client = reqwest::Client::new();
    let mut req = client.get(&url).header("User-Agent", "axon-refresh");
    if let Some(token) = &cfg.github_token {
        req = req.header("Authorization", format!("Bearer {token}"));
    }
    let resp: serde_json::Value = req.send().await?.json().await?;
    resp["pushed_at"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| "missing pushed_at in GitHub API response".into())
}

/// Pure comparison: should we re-ingest based on pushed_at vs last_run_at?
fn should_reingest_github(pushed_at: &str, last_run_at: Option<DateTime<Utc>>) -> bool {
    let Some(last) = last_run_at else { return true };
    let Ok(pushed) = chrono::DateTime::parse_from_rfc3339(pushed_at) else { return true };
    pushed.with_timezone(&Utc) > last
}
```

- [ ] **Step 6: Write a unit test for the changed-detection logic**

Create a pure function `should_reingest_github(pushed_at: &str, last_run_at: Option<DateTime<Utc>>) -> bool` and test it:

```rust
#[test]
fn reingest_when_pushed_after_last_run() {
    let pushed = "2026-03-10T12:00:00Z";
    let last_run = Some(Utc.with_ymd_and_hms(2026, 3, 10, 10, 0, 0).unwrap());
    assert!(should_reingest_github(pushed, last_run));
}

#[test]
fn skip_when_no_push_since_last_run() {
    let pushed = "2026-03-10T08:00:00Z";
    let last_run = Some(Utc.with_ymd_and_hms(2026, 3, 10, 10, 0, 0).unwrap());
    assert!(!should_reingest_github(pushed, last_run));
}

#[test]
fn reingest_on_first_run() {
    let pushed = "2026-03-10T12:00:00Z";
    assert!(should_reingest_github(pushed, None));
}
```

- [ ] **Step 7: Fix ALL existing `RefreshScheduleCreate` literals**

Grep for `RefreshScheduleCreate {` and add the new fields with `None`:

```rust
source_type: None,
target: None,
```

**Known locations (both production and test code):**
- `crates/jobs/refresh.rs`: 4 instances (lines ~374, 387, 400, 466 — all in tests)
- `crates/mcp/server/handlers_refresh_status.rs`: line ~175 (production — MCP handler)
- `crates/cli/commands/refresh/schedule.rs`: line ~156 (production — CLI handler)

**All 6 must be updated** — missing any one will be a compile error.

- [ ] **Step 8: Verify everything compiles and tests pass**

Run: `cargo check && cargo test --lib 2>&1 | tail -5`

- [ ] **Step 9: Commit**

```bash
git add crates/jobs/refresh.rs crates/jobs/refresh/schedule.rs
git commit -m "$(cat <<'EOF'
feat(refresh): extend RefreshSchedule for GitHub repo re-ingestion

New source_type + target columns on axon_refresh_schedules.
Schedule tick checks pushed_at via GitHub API, only re-ingests
when the repo has been pushed since last run.
EOF
)"
```

---

### Task 13: Wire `axon refresh schedule github:owner/repo`

**Files:**
- Modify: `crates/cli/commands/refresh/schedule.rs` (function that handles `schedule` subcommand, ~line 156)

- [ ] **Step 1: Detect `github:` prefix in schedule target**

When the user runs `axon refresh schedule github:owner/repo --every 6h`, parse the target:

```rust
if let Some(repo) = target.strip_prefix("github:") {
    // Validate it looks like owner/repo
    if !repo.contains('/') || repo.split('/').count() != 2 {
        return Err("Invalid GitHub target. Expected: github:owner/repo".into());
    }
    let create = RefreshScheduleCreate {
        name: name.unwrap_or_else(|| repo.replace('/', "-")),
        seed_url: None,
        urls: None,
        every_seconds,
        enabled: true,
        next_run_at: Utc::now(),
        source_type: Some("github".to_string()),
        target: Some(repo.to_string()),
    };
    // ...
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`

- [ ] **Step 3: Commit**

```bash
git add crates/cli/commands/refresh.rs
git commit -m "$(cat <<'EOF'
feat(cli): axon refresh schedule github:owner/repo --every 6h

Parses github: prefix, stores source_type=github + target=owner/repo
on the refresh schedule. Worker picks up and does pushed_at comparison.
EOF
)"
```

---

### Task 14: Final Integration Check + Cleanup

**Files:**
- All modified files

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1 | tail -10`
Expected: all tests pass, no regressions

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1 | tail -10`
Expected: 0 warnings

- [ ] **Step 3: Run fmt check**

Run: `cargo fmt --check 2>&1 | tail -5`
Expected: clean

- [ ] **Step 4: Run monolith check**

Run: `python3 scripts/enforce_monoliths.py 2>&1 | tail -5`
Expected: no violations (all new files are well under 500 lines)

- [ ] **Step 5: Commit any fmt fixes**

```bash
cargo fmt
git add -A
git commit -m "style: fmt + clippy fixes"
```

---

## Summary

| Task | Component | New/Modified Files | Tests |
|------|-----------|-------------------|-------|
| 1 | Dependencies | `Cargo.toml` | compile check |
| 2 | File classification | `input/classify.rs`, `input.rs` | 27 unit tests |
| 3 | Code chunker | `input/code.rs`, `input.rs` | 18 unit tests |
| 4 | Embed pipeline | `tei.rs` | existing tests (refactor) |
| 5 | Unified payload | `meta.rs` | 6 unit tests |
| 6 | Orchestration | `github.rs` | compile check |
| 7 | Files integration | `files.rs` | compile + existing |
| 8 | Issues integration | `issues.rs`, `meta.rs` | compile + existing |
| 9 | Wiki integration | `wiki.rs` | compile + existing |
| 10 | Remove old builders | `meta.rs` | grep check |
| 11 | Default flip | `config/` (4 files) | existing config tests |
| 12 | Refresh schedule | `refresh.rs`, `schedule.rs` | 3 unit tests |
| 13 | CLI wire | `refresh.rs` (cli) | compile check |
| 14 | Final check | all | full suite + clippy + fmt + monolith |
