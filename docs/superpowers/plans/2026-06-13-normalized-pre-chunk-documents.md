# Normalized Pre-Chunk Documents Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a normalized pre-chunk document boundary so Axon acquisition paths hand raw source documents to one shared planner before TEI/Qdrant embedding.

**Architecture:** `SourceDocument` is the normalized pre-chunk type and `PreparedDoc` remains the post-chunk, embed-ready type. `SourceDocument` carries an explicit origin and chunk strategy so crawl manifests, scrape results, local files, git files, and ingest text cannot be confused. The planner owns chunk selection, safe markdown fallback, typed per-chunk metadata, locator/range generation, and metadata parity. TEI/Qdrant only embeds prepared chunks and stamps payloads.

**Tech Stack:** Rust 2024, Tokio, serde_json, SpiderUrl, existing Axon chunkers (`chunk_file`, `chunk_markdown`, `chunk_text`), existing `apply_extra` merge path, existing TEI/Qdrant embed pipeline.

---

## Engineering Review Changes Applied

- Preserve current control-character markdown fallback by adding `safe_markdown_chunks()` in the planner.
- Use one file-path chunker: `SourceChunkHint::File { path, extension }` delegates to existing `chunk_file()`.
- Add explicit `SourceOrigin` so HTTP crawl/scrape URLs never take local code chunking paths.
- Preserve existing `code_*`, `git_*`, provider, session, Reddit, and YouTube payload keys additively.
- Replace raw `chunk_extra` construction for normalized fields with typed `ChunkMetadata` generated only by the planner.
- Do not add a duplicate payload merge helper; test the existing `apply_extra` behavior.
- Cap/redact vertical structured payloads before they appear in public `ScrapeResult`.
- Bump `PAYLOAD_SCHEMA_VERSION` and document that new fields are optional on old points.
- Add bounded planning and byte-budget invariants so the new boundary does not become an eager memory choke point.
- Replace the brittle source-string chunk-call audit with API privacy, targeted behavior tests, and a narrow import/constructor guard.
- Include GitHub wiki and top-level GitHub repo docs in the migration scope.
- Update `src/ingest/CLAUDE.md` and vector docs so local instructions match the new boundary.

## File Structure

- Create `src/vector/ops/source_doc.rs`: owns `SourceDocument`, `SourceOrigin`, `SourceChunkHint`, typed `ChunkMetadata`, source constructors, safe markdown fallback, bounded planning helpers, and conversion to `PreparedDoc`.
- Create `src/vector/ops/source_doc_tests.rs`: planner tests for markdown fallback, file chunking, code metadata parity, vertical structured payloads, and bounded planning.
- Create `src/vector/ops/source_doc_audit_tests.rs`: narrow guard tests for forbidden imports/manual constructors after the privacy changes.
- Modify `src/vector/ops.rs`: export only the source document API needed by callers.
- Modify `src/vector/ops/tei.rs`: keep `PreparedDoc` post-chunk only and restrict construction to planner-owned validated chunks.
- Modify `src/vector/ops/tei/pipeline.rs`: reuse existing `apply_extra`, bump payload schema, and keep new normalized fields optional for old points.
- Modify `src/vector/ops/tei/prepare.rs`: local file/directory embeds and crawl-manifest embeds create `SourceDocument`s with explicit origins.
- Modify `src/services/types/service/content.rs`: add capped/redacted vertical structured summary fields to `ScrapeResult`.
- Modify `src/services/scrape.rs`: convert generic and vertical scrape results into `SourceDocument`s without changing returned output format semantics.
- Modify git ingest files: `src/ingest/github/files/prepare.rs`, `src/ingest/github/wiki.rs`, `src/ingest/github.rs`, `src/ingest/gitlab/files.rs`, and `src/ingest/generic_git.rs`.
- Modify plain-text ingest builders only through sync `SourceDocument` constructors or a `prepare_plain_text_source` helper, preserving existing streaming/batch boundaries: `src/ingest/reddit.rs`, `src/ingest/youtube.rs`, `src/ingest/github/issues.rs`, `src/ingest/gitlab/embed.rs`, `src/ingest/gitea/embed.rs`, `src/ingest/sessions/prepared.rs`, `src/ingest/sessions/claude.rs`, `src/ingest/sessions/codex.rs`, and `src/ingest/sessions/gemini.rs`.
- Modify docs: `src/ingest/CLAUDE.md` and `src/vector/CLAUDE.md`.

## Locked Invariants

- Existing Qdrant collections do not receive an in-place full migration.
- New local code embeds use one file-level `PreparedDoc`; line-specific navigation lives in `chunk_locator` and `source_range`.
- Local path locators must be relative to the embed root when possible; public payloads must not expose more absolute path detail than the existing URL already exposes.
- Old local code fragment points must be cleaned up by URL prefix or `code_file_path` before writing the new file-level doc.
- Only `SourceOrigin::LocalFile` and git file origins may use `SourceChunkHint::File`.
- Crawl manifest and scrape results always use `MarkdownOrPlainText`, preserving control-character fallback.
- New normalized payload fields are trusted planner output. Callers cannot set them through doc-level `extra`.
- Planner and provider adapters must preserve existing payload keys; new fields are additive.
- Existing streaming/backpressure shapes, especially Reddit’s bounded drain, must remain bounded.
- Planner batching must obey both document-count and byte-budget limits.
- Structured blobs must be capped before public serialization and measured against per-chunk write amplification before embedding.

## Task 1: Add SourceDocument Types, Constructors, And Safe Planner

**Files:**
- Create: `src/vector/ops/source_doc.rs`
- Create: `src/vector/ops/source_doc_tests.rs`
- Modify: `src/vector/ops.rs`
- Modify: `src/vector/ops/tei.rs`

- [ ] **Step 1: Write failing planner type tests**

Add `src/vector/ops/source_doc_tests.rs`:

```rust
use super::source_doc::{
    SourceChunkHint, SourceDocument, SourceOrigin, prepare_source_document,
};
use super::tei::StructuredPayload;

#[tokio::test]
async fn markdown_with_control_chars_falls_back_to_plain_text_chunking() {
    let source = SourceDocument::try_new_web_markdown(
        "https://example.com/control".to_string(),
        "# Title\n\nbad\u{0008}content".to_string(),
        "scrape",
        None,
        None,
        None,
        None,
    )
    .expect("source doc");

    let prepared = prepare_source_document(source).await.expect("prepared doc");

    assert_eq!(prepared.content_type, "markdown");
    assert_eq!(prepared.chunk_extra.len(), prepared.chunks.len());
    assert_eq!(prepared.chunk_extra[0]["content_kind"], "markdown");
    assert_eq!(prepared.chunk_extra[0]["chunking_fallback"], "plain_text_control_chars");
}

#[tokio::test]
async fn crawl_manifest_rs_url_does_not_use_code_chunking() {
    let source = SourceDocument::try_new_crawl_manifest(
        "https://example.com/src/lib.rs".to_string(),
        "fn looks_like_code() {}\n".to_string(),
        None,
        None,
    )
    .expect("source doc");

    let prepared = prepare_source_document(source).await.expect("prepared doc");

    assert_eq!(prepared.content_type, "markdown");
    assert_eq!(prepared.chunk_extra[0]["content_kind"], "markdown");
    assert!(prepared.chunk_extra[0].get("code_line_start").is_none());
}

#[tokio::test]
async fn file_source_attaches_existing_code_keys_and_new_locator_keys() {
    let source = SourceDocument::try_new_file(
        SourceOrigin::GitFile,
        "https://github.com/owner/repo/blob/main/src/lib.rs".to_string(),
        "src/lib.rs".to_string(),
        "rs".to_string(),
        "pub struct Parser;\nimpl Parser { pub fn parse(&self) {} }\n".to_string(),
        "github",
        Some("src/lib.rs".to_string()),
        Some(serde_json::json!({
            "provider": "github",
            "git_owner": "owner",
            "git_repo": "repo",
            "git_content_kind": "file",
            "code_file_path": "src/lib.rs",
            "code_language": "rust",
            "code_is_test": false
        })),
    )
    .expect("source doc");

    let prepared = prepare_source_document(source).await.expect("prepared doc");

    assert_eq!(prepared.url, "https://github.com/owner/repo/blob/main/src/lib.rs");
    assert_eq!(prepared.chunks.len(), prepared.chunk_extra.len());
    let doc_extra = prepared.extra.as_ref().expect("doc extra");
    assert_eq!(doc_extra["git_owner"], "owner");
    assert_eq!(doc_extra["code_language"], "rust");
    let chunk_extra = prepared
        .chunk_extra
        .iter()
        .find(|extra| extra.get("symbol_name").and_then(|v| v.as_str()) == Some("Parser::parse"))
        .expect("method symbol");
    assert_eq!(chunk_extra["content_kind"], "code");
    assert!(chunk_extra["chunk_locator"].as_str().unwrap().contains("src/lib.rs#L"));
    assert!(chunk_extra["source_range"]["line_start"].as_u64().is_some());
    assert!(chunk_extra["code_line_start"].as_u64().is_some());
    assert!(chunk_extra["code_line_end"].as_u64().is_some());
    assert!(chunk_extra["code_chunking_method"].as_str().is_some());
}

#[tokio::test]
async fn source_document_preserves_vertical_structured_payload() {
    let source = SourceDocument::try_new_web_markdown(
        "https://pypi.org/project/ruff/".to_string(),
        "# ruff\n\nFast Python linter.".to_string(),
        "scrape",
        Some("ruff".to_string()),
        Some(serde_json::json!({"pkg_name": "ruff"})),
        Some("pypi".to_string()),
        Some(StructuredPayload {
            kind: "vertical",
            schema_type: Some("pypi_structured".to_string()),
            schema_id: Some("ruff".to_string()),
            blob: serde_json::json!({"name": "ruff"}),
        }),
    )
    .expect("source doc");

    let prepared = prepare_source_document(source).await.expect("prepared doc");

    assert_eq!(prepared.extractor_name.as_deref(), Some("pypi"));
    assert_eq!(prepared.extra.as_ref().unwrap()["pkg_name"], "ruff");
    assert_eq!(
        prepared.structured.as_ref().unwrap().schema_id.as_deref(),
        Some("ruff")
    );
}
```

- [ ] **Step 2: Run planner tests and confirm they fail**

Run:

```bash
cargo test -p axon --lib vector::ops::source_doc
```

Expected: fail because `source_doc` module and constructors do not exist.

- [ ] **Step 3: Implement public source-document API**

Add `src/vector/ops/source_doc.rs` with these public types:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceOrigin {
    LocalFile,
    GitFile,
    CrawlManifest,
    ScrapeResult,
    PlainIngest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceChunkHint {
    File { path: String, extension: String },
    MarkdownOrPlainText,
    PlainText,
}

#[derive(Debug, Clone)]
pub(crate) struct SourceDocument {
    origin: SourceOrigin,
    url: String,
    domain: String,
    text: String,
    source_type: String,
    title: Option<String>,
    extra: Option<serde_json::Value>,
    extractor_name: Option<String>,
    structured: Option<crate::vector::ops::tei::StructuredPayload>,
    chunk_hint: SourceChunkHint,
}
```

Expose read-only methods for tests and adapters that need to inspect source metadata:

```rust
impl SourceDocument {
    pub(crate) fn url(&self) -> &str { &self.url }
    pub(crate) fn estimated_bytes(&self) -> usize {
        self.text.len()
            + self.url.len()
            + self.title.as_ref().map_or(0, String::len)
            + self.extra.as_ref().map_or(0, |v| v.to_string().len())
    }
}
```

- [ ] **Step 4: Implement validating constructors**

Implement constructors with these rules:

```rust
impl SourceDocument {
    pub(crate) fn try_new_web_markdown(
        url: String,
        text: String,
        source_type: impl Into<String>,
        title: Option<String>,
        extra: Option<serde_json::Value>,
        extractor_name: Option<String>,
        structured: Option<StructuredPayload>,
    ) -> Result<Self, String> {
        let domain = domain_from_web_url(&url)?;
        Ok(Self {
            origin: SourceOrigin::ScrapeResult,
            url,
            domain,
            text,
            source_type: source_type.into(),
            title,
            extra: sanitize_doc_extra(extra),
            extractor_name,
            structured,
            chunk_hint: SourceChunkHint::MarkdownOrPlainText,
        })
    }

    pub(crate) fn try_new_crawl_manifest(
        url: String,
        text: String,
        title: Option<String>,
        structured: Option<StructuredPayload>,
    ) -> Result<Self, String> {
        let domain = domain_from_web_url(&url)?;
        Ok(Self {
            origin: SourceOrigin::CrawlManifest,
            url,
            domain,
            text,
            source_type: "crawl".to_string(),
            title,
            extra: None,
            extractor_name: None,
            structured,
            chunk_hint: SourceChunkHint::MarkdownOrPlainText,
        })
    }

    pub(crate) fn try_new_file(
        origin: SourceOrigin,
        url: String,
        path: String,
        extension: String,
        text: String,
        source_type: impl Into<String>,
        title: Option<String>,
        extra: Option<serde_json::Value>,
    ) -> Result<Self, String> {
        if !matches!(origin, SourceOrigin::LocalFile | SourceOrigin::GitFile) {
            return Err("file chunking is only allowed for local and git file origins".to_string());
        }
        let domain = domain_for_origin(origin, &url);
        Ok(Self {
            origin,
            url,
            domain,
            text,
            source_type: source_type.into(),
            title,
            extra: sanitize_doc_extra(extra),
            extractor_name: None,
            structured: None,
            chunk_hint: SourceChunkHint::File { path, extension },
        })
    }

    pub(crate) fn try_new_plain_text(
        url: String,
        domain: String,
        text: String,
        source_type: impl Into<String>,
        title: Option<String>,
        extra: Option<serde_json::Value>,
    ) -> Result<Self, String> {
        Ok(Self {
            origin: SourceOrigin::PlainIngest,
            url,
            domain,
            text,
            source_type: source_type.into(),
            title,
            extra: sanitize_doc_extra(extra),
            extractor_name: None,
            structured: None,
            chunk_hint: SourceChunkHint::PlainText,
        })
    }
}
```

`sanitize_doc_extra` must remove normalized planner-owned keys from caller-supplied objects:

```rust
const PLANNER_OWNED_PAYLOAD_KEYS: &[&str] = &[
    "content_kind",
    "chunk_locator",
    "source_range",
    "chunking_fallback",
    "code_chunk_source",
];
```

Do not remove existing provider fields such as `git_*`, `code_*`, `reddit_*`, `yt_*`, and session fields.

- [ ] **Step 5: Implement safe chunk planning**

Implement:

```rust
pub(crate) async fn prepare_source_document(doc: SourceDocument) -> Result<PreparedDoc, String>;
```

Rules:

- `SourceChunkHint::File` delegates to `chunk_file(&text, &extension)` inside `spawn_blocking`.
- Compute `code_chunking_method` per chunk using the current signature: `chunking_method(&extension, chunk)`.
- Preserve existing doc-level `extra`; add new per-chunk fields additively.
- `SourceChunkHint::MarkdownOrPlainText` uses `safe_markdown_chunks(&doc.text)`.
- `safe_markdown_chunks` uses `chunk_text` when text has control characters other than newline, carriage return, or tab; otherwise it uses `chunk_markdown`.
- `SourceChunkHint::PlainText` uses `chunk_text`.

Planner-created per-chunk metadata must include:

```json
{
  "content_kind": "code|markdown|plain_text",
  "chunk_locator": "stable locator",
  "source_range": { "line_start": 1, "line_end": 3, "byte_start": 0, "byte_end": 120 }
}
```

For file chunks, preserve existing keys too:

```json
{
  "code_line_start": 1,
  "code_line_end": 3,
  "code_chunking_method": "tree_sitter|markdown|prose",
  "code_chunk_source": "tree_sitter|markdown|prose",
  "symbol_name": "...",
  "symbol_kind": "...",
  "symbol_path": "..."
}
```

- [ ] **Step 6: Restrict `PreparedDoc` construction**

In `src/vector/ops/tei.rs`, add a constructor used by the planner:

```rust
pub(super) fn from_planned_chunks(
    url: String,
    domain: String,
    chunks: Vec<String>,
    source_type: impl Into<String>,
    content_type: &'static str,
    title: Option<String>,
    extra: Option<serde_json::Value>,
    extractor_name: Option<String>,
    structured: Option<StructuredPayload>,
    chunk_extra: Vec<serde_json::Value>,
) -> PreparedDoc { ... }
```

If module privacy makes `pub(super)` inaccessible from `source_doc.rs`, put `source_doc.rs` under `src/vector/ops/tei/source_doc.rs` instead and re-export the source API through `vector::ops`. The constructor must not be `pub(crate)`.

- [ ] **Step 7: Export only source document API**

In `src/vector/ops.rs`, export:

```rust
pub(crate) use source_doc::{
    SourceDocument, SourceOrigin, prepare_source_document,
    prepare_source_documents_bounded,
};
```

Do not export `ChunkMetadata` or planner-owned key constants.

- [ ] **Step 8: Run planner tests**

Run:

```bash
cargo test -p axon --lib vector::ops::source_doc
```

Expected: pass.

## Task 2: Payload Merge, Schema, And Compatibility

**Files:**
- Modify: `src/vector/ops/tei/pipeline.rs`
- Modify: `src/vector/ops/tei/pipeline_tests.rs`
- Modify: `src/vector/ops/qdrant/utils.rs`
- Modify docs: `src/vector/CLAUDE.md`

- [ ] **Step 1: Write failing merge and schema tests**

Add to `src/vector/ops/tei/pipeline_tests.rs`:

```rust
#[test]
fn apply_extra_allows_planner_chunk_fields_but_blocks_system_fields() {
    let mut payload = serde_json::json!({
        "url": "https://example.com/original",
        "chunk_text": "original"
    });
    let extra = serde_json::json!({
        "url": "https://evil.example/override",
        "chunk_text": "evil",
        "content_kind": "code",
        "chunk_locator": "src/lib.rs#L1-L2",
        "source_range": {"line_start": 1, "line_end": 2}
    });

    super::apply_extra(&mut payload, &extra);

    assert_eq!(payload["url"], "https://example.com/original");
    assert_eq!(payload["chunk_text"], "original");
    assert_eq!(payload["content_kind"], "code");
    assert_eq!(payload["chunk_locator"], "src/lib.rs#L1-L2");
    assert_eq!(payload["source_range"]["line_start"], 1);
}

#[test]
fn payload_schema_version_covers_source_document_fields() {
    assert!(
        crate::vector::ops::qdrant::utils::PAYLOAD_SCHEMA_VERSION >= 8,
        "SourceDocument normalized fields require a schema bump"
    );
}
```

- [ ] **Step 2: Run tests and confirm schema failure**

Run:

```bash
cargo test -p axon --lib vector::ops::tei::pipeline_tests::apply_extra_allows_planner_chunk_fields_but_blocks_system_fields vector::ops::tei::pipeline_tests::payload_schema_version_covers_source_document_fields
```

Expected: fail until `apply_extra` is visible to tests and schema version is bumped.

- [ ] **Step 3: Reuse existing `apply_extra`**

Do not add a new merge helper. Make `apply_extra` visible to the test module if necessary with `pub(super)`.

- [ ] **Step 4: Bump payload schema**

In `src/vector/ops/qdrant/utils.rs`, bump `PAYLOAD_SCHEMA_VERSION` from `7` to `8`. Update the adjacent doc-comment to list v8 fields:

- `content_kind`
- `chunk_locator`
- `source_range`
- `chunking_fallback`
- `code_chunk_source`

- [ ] **Step 5: Document optional compatibility**

In `src/vector/CLAUDE.md`, add that v8 fields are optional in mixed collections and consumers must not assume they exist unless filtering by `payload_schema_version >= 8`.

- [ ] **Step 6: Run pipeline/schema tests**

Run:

```bash
cargo test -p axon --lib vector::ops::tei::pipeline_tests
```

Expected: pass.

## Task 3: Preserve Vertical Structured Payloads Safely

**Files:**
- Modify: `src/services/types/service/content.rs`
- Modify: `src/services/scrape.rs`
- Modify: `src/services/scrape_tests.rs`

- [ ] **Step 1: Write failing vertical preservation and cap tests**

Add to `src/services/scrape_tests.rs`:

```rust
#[test]
fn vertical_doc_to_scrape_result_preserves_capped_structured_summary() {
    let result = super::vertical_doc_to_scrape_result(crate::extract::ScrapedDoc {
        url: "https://pypi.org/project/ruff/".to_string(),
        markdown: "# ruff\n\nFast Python linter.".to_string(),
        title: Some("ruff".to_string()),
        extractor_name: "pypi",
        extractor_version: 3,
        structured: Some(serde_json::json!({
            "name": "ruff",
            "api_token": "secret-value-that-must-not-leak"
        })),
        follow_crawl_urls: vec!["https://docs.astral.sh/ruff/".to_string()],
        extra: Some(serde_json::json!({"pkg_name": "ruff"})),
    })
    .expect("scrape result");

    assert_eq!(result.extractor_name.as_deref(), Some("pypi"));
    assert_eq!(result.extra.as_ref().unwrap()["extractor_version"], 3);
    let structured = result.structured.as_ref().expect("structured summary");
    assert_eq!(structured["name"], "ruff");
    assert!(structured.get("api_token").is_none());
}

#[test]
fn vertical_structured_summary_drops_oversized_payload() {
    let large = "x".repeat(crate::services::scrape::MAX_PUBLIC_STRUCTURED_BYTES + 1);
    let result = super::vertical_doc_to_scrape_result(crate::extract::ScrapedDoc {
        url: "https://example.com/large".to_string(),
        markdown: "# Large".to_string(),
        title: None,
        extractor_name: "example",
        extractor_version: 1,
        structured: Some(serde_json::json!({"large": large})),
        follow_crawl_urls: Vec::new(),
        extra: None,
    })
    .expect("scrape result");

    assert!(result.structured.is_none());
}
```

Add a format regression test that builds a vertical-like `ScrapeResult` with `markdown = "# Package\n\nbody"` and `output = "<article>Package</article>"`, then calls the new scrape-result-to-source-document adapter directly. Assert the resulting `SourceDocument` text is `"# Package\n\nbody"` and the original `ScrapeResult.output` remains `"<article>Package</article>"`.

- [ ] **Step 2: Add public structured summary field**

In `src/services/types/service/content.rs`, add:

```rust
/// Redacted and size-capped structured data summary from a vertical extractor.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub structured: Option<serde_json::Value>,
```

This field is for API/artifact output only. Embedding may use the same redacted value, never raw unbounded extractor JSON.

- [ ] **Step 3: Add redaction and cap helper in scrape service**

In `src/services/scrape.rs`, add:

```rust
pub(crate) const MAX_PUBLIC_STRUCTURED_BYTES: usize = 16 * 1024;

fn public_structured_summary(value: serde_json::Value) -> Option<serde_json::Value> {
    let redacted = redact_sensitive_structured_keys(value);
    let bytes = serde_json::to_vec(&redacted).ok()?;
    if bytes.len() > MAX_PUBLIC_STRUCTURED_BYTES {
        None
    } else {
        Some(redacted)
    }
}
```

`redact_sensitive_structured_keys` must recursively remove object keys whose lowercase name contains `token`, `secret`, `password`, `authorization`, `cookie`, or `api_key`.

- [ ] **Step 4: Preserve extractor version and structured summary**

In `vertical_doc_to_scrape_result`, add `extractor_version` into `extra` and set `scrape_result.structured = doc.structured.and_then(public_structured_summary)`.

- [ ] **Step 5: Convert scrape results through `SourceDocument`**

Change `scrape_result_to_prepared_doc` to async and build a `SourceDocument::try_new_web_markdown(...)`. Convert `result.structured` into `StructuredPayload` with a helper in `source_doc.rs`, not in TEI:

```rust
pub(crate) fn structured_payload_from_vertical_summary(
    extractor_name: &str,
    value: serde_json::Value,
    max_bytes: usize,
) -> Option<StructuredPayload>
```

This helper must use the already-redacted public summary and respect `cfg.structured_data_max_bytes`.

- [ ] **Step 6: Keep vertical output format semantics**

Do not make embedding consume `ScrapeResult.output`. Embedding must consume curated `ScrapeResult.markdown`. Add a test proving a vertical result with non-markdown public output still embeds the markdown field.

- [ ] **Step 7: Run scrape tests**

Run:

```bash
cargo test -p axon --lib services::scrape_tests
```

Expected: pass.

## Task 4: Route Local Embeds And Crawl Manifests Through SourceDocument

**Files:**
- Modify: `src/vector/ops/tei/prepare.rs`
- Modify: `src/vector/ops/tei/prepare_tests.rs`

- [ ] **Step 1: Update local embed expectations**

Update `dir_embed_tags_code_and_prose_distinctly` so `lib.rs` produces one file-level `PreparedDoc` with parallel `chunks` and `chunk_extra`, not one doc per chunk:

```rust
assert_eq!(rs.content_type, "text");
assert!(rs.chunks.len() >= 1);
assert_eq!(rs.chunks.len(), rs.chunk_extra.len());
assert_eq!(rs.chunk_extra[0]["content_kind"], "code");
assert!(rs.chunk_extra[0]["chunk_locator"].as_str().unwrap().contains("lib.rs#L"));
assert!(rs.chunk_extra[0]["code_line_start"].as_u64().is_some());
```

Add a regression test:

```rust
#[tokio::test]
async fn crawl_manifest_rs_url_stays_markdown_not_code() {
    // manifest maps local markdown file to https://example.com/src/lib.rs
    // expected: content_type markdown, content_kind markdown, no code_line_start
}
```

Add a regression test for control-character markdown:

```rust
#[tokio::test]
async fn crawl_manifest_markdown_with_control_chars_does_not_panic() {
    // expected: prepared doc exists and chunk_extra has chunking_fallback
}
```

- [ ] **Step 2: Run local tests and confirm failure**

Run:

```bash
cargo test -p axon --lib vector::ops::tei::prepare_tests::dir_embed_tags_code_and_prose_distinctly vector::ops::tei::prepare_tests::crawl_manifest_rs_url_stays_markdown_not_code vector::ops::tei::prepare_tests::crawl_manifest_markdown_with_control_chars_does_not_panic
```

Expected: fail until local embed uses `SourceDocument`.

- [ ] **Step 3: Build source docs with explicit origins**

In `prepare_embed_docs`, map inputs:

- crawl manifest entries -> `SourceDocument::try_new_crawl_manifest`
- local filesystem file entries -> `SourceDocument::try_new_file(SourceOrigin::LocalFile, ...)`
- remote HTTP URL direct embed -> `SourceDocument::try_new_web_markdown`

Do not call `should_chunk_as_code()` for crawl-manifest URLs.

- [ ] **Step 4: Redact local locator paths**

For local embeds, pass `path` to `try_new_file` as a path relative to the embed root when the input is a directory. If a single file is embedded directly, use the file name as locator path. Keep the existing `url` behavior unchanged for compatibility, but `chunk_locator` must prefer the relative locator path.

- [ ] **Step 5: Clean old local fragment points**

Before embedding a new local file-level code doc, delete old points whose URL starts with the old per-chunk prefix:

```text
{file_url}#L
```

If existing payloads have `code_file_path`, also delete matching points by `code_file_path` when present. This cleanup is only for local file embeds where URL identity changes from per-chunk to file-level. Do not delete git provider points here.

- [ ] **Step 6: Remove obsolete local helpers**

Delete local `select_chunks()` and `embed_code_file_docs()` only after all tests pass through the planner.

- [ ] **Step 7: Run local embed tests**

Run:

```bash
cargo test -p axon --lib vector::ops::tei::prepare_tests
```

Expected: pass.

## Task 5: Route Git File Providers And GitHub Docs Through SourceDocument

**Files:**
- Modify: `src/ingest/github/files/prepare.rs`
- Modify: `src/ingest/github/files/prepare_tests.rs`
- Modify: `src/ingest/github/wiki.rs`
- Modify: `src/ingest/github.rs`
- Modify: `src/ingest/gitlab/files.rs`
- Modify: `src/ingest/gitlab/files_tests.rs`
- Modify: `src/ingest/generic_git.rs`
- Modify: `src/ingest/generic_git_tests.rs`

- [ ] **Step 1: Add metadata parity assertions**

In GitHub file tests, assert all existing doc-level keys still survive:

```rust
let extra = doc.extra.as_ref().expect("github payload");
assert_eq!(extra["provider"], "github");
assert_eq!(extra["git_content_kind"], "file");
assert_eq!(extra["code_file_path"], "src/lib.rs");
assert_eq!(extra["code_language"], "rust");
assert!(extra.get("code_is_test").is_some());
assert_eq!(doc.chunks.len(), doc.chunk_extra.len());
assert_eq!(doc.chunk_extra[0]["content_kind"], "code");
assert!(doc.chunk_extra[0]["chunk_locator"].as_str().unwrap().contains("src/lib.rs#L"));
assert!(doc.chunk_extra[0]["code_line_start"].as_u64().is_some());
```

Add a test for markdown file ingest (`README.md`) proving `content_kind=markdown`, `code_chunk_source=markdown`, and existing git payload keys remain.

- [ ] **Step 2: Run git file tests and confirm failure**

Run:

```bash
cargo test -p axon --lib ingest::github::files
```

Expected: fail until GitHub file ingest uses planner metadata.

- [ ] **Step 3: Replace direct GitHub file chunking**

In `read_file_embed_docs`, build doc-level `extra` exactly as today before calling the planner. Then call:

```rust
let source_doc = crate::vector::ops::SourceDocument::try_new_file(
    crate::vector::ops::SourceOrigin::GitFile,
    url,
    rel.to_string(),
    ext_for_chunk,
    text,
    "github",
    Some(rel.to_string()),
    Some(extra),
)
.map_err(|err| format!("invalid source document for {rel}: {err}"))?;

let doc = crate::vector::ops::prepare_source_document(source_doc)
    .await
    .map_err(|err| format!("prepare source document failed for {rel}: {err}"))?;
Ok(vec![doc])
```

- [ ] **Step 4: Repeat for GitLab and generic Git**

Convert `src/ingest/gitlab/files.rs` and `src/ingest/generic_git.rs` the same way. Keep each provider’s existing doc-level payload fields unchanged. Add provider-specific parity tests for at least one source file and one markdown file.

- [ ] **Step 5: Route GitHub wiki and top-level repo docs**

Convert `src/ingest/github/wiki.rs` and top-level repo docs in `src/ingest/github.rs` from direct `chunk_markdown` calls to `SourceDocument::try_new_web_markdown` or `try_new_plain_text` based on their current URL/source semantics. Preserve existing `source_type` and metadata.

- [ ] **Step 6: Flush git providers by batch**

Do not introduce a whole-repo `Vec<SourceDocument>` accumulation. Preserve existing GitHub batching. For GitLab and generic Git, replace the final single all-doc embed call with a local `flush_git_batch` helper that flushes during traversal. Flush by both:

- file count, using the existing provider batch size when present
- estimated bytes, using `SourceDocument::estimated_bytes()`

- [ ] **Step 7: Run git provider tests**

Run:

```bash
cargo test -p axon --lib ingest::github ingest::gitlab ingest::generic_git
```

Expected: pass.

## Task 6: Plain Text Ingest Without Async Churn

**Files:**
- Modify: `src/ingest/reddit.rs`
- Modify: `src/ingest/youtube.rs`
- Modify: `src/ingest/github/issues.rs`
- Modify: `src/ingest/gitlab/embed.rs`
- Modify: `src/ingest/gitea/embed.rs`
- Modify: `src/ingest/sessions/prepared.rs`
- Modify: `src/ingest/sessions/claude.rs`
- Modify: `src/ingest/sessions/codex.rs`
- Modify: `src/ingest/sessions/gemini.rs`
- Update matching tests.

- [ ] **Step 1: Add sync plain-text helper**

In `source_doc.rs`, add:

```rust
pub(crate) fn prepare_plain_text_source(
    url: String,
    domain: String,
    text: String,
    source_type: impl Into<String>,
    title: Option<String>,
    extra: Option<serde_json::Value>,
) -> Result<PreparedDoc, String>
```

This helper builds `SourceDocument::try_new_plain_text(...)` and prepares it synchronously because plain text uses `chunk_text` and does not need `spawn_blocking`.

- [ ] **Step 2: Preserve existing streaming shapes**

Update Reddit so each post prepares one doc before sending to the existing bounded drain, or change the bounded drain channel to carry `SourceDocument` and prepare/flush there. The implementation must keep the bounded channel and batch flushing from `src/ingest/reddit.rs`.

Update sessions by converting `PreparedSessionDoc::to_prepared_doc()` to either:

```rust
pub(crate) fn to_source_document(&self) -> Result<SourceDocument, String>
```

or directly:

```rust
pub(crate) fn to_prepared_doc(&self) -> Result<PreparedDoc, String>
```

using `prepare_plain_text_source`. Keep the current reserved-extra filtering in `src/ingest/sessions/prepared.rs`.

- [ ] **Step 3: Add parity tests**

For Reddit, YouTube, GitHub issues, GitLab/Gitea metadata docs, and sessions, add or update tests to assert:

- existing provider-specific keys survive
- `content_kind=plain_text`
- chunks and `chunk_extra` lengths match
- no provider builder needs to become async only to call plain-text chunking

- [ ] **Step 4: Run targeted ingest tests**

Run:

```bash
cargo test -p axon --lib ingest::reddit ingest::youtube ingest::github::issues ingest::gitlab ingest::gitea ingest::sessions
```

Expected: pass.

## Task 7: Bounded Planning And Batch Flush Invariants

**Files:**
- Modify: `src/vector/ops/source_doc.rs`
- Modify: `src/vector/ops/source_doc_tests.rs`
- Modify provider batching code touched in Tasks 3-6.

- [ ] **Step 1: Add bounded planner API**

Replace eager all-doc planning API with:

```rust
pub(crate) async fn prepare_source_documents_bounded<I>(
    docs: I,
    concurrency: usize,
    max_inflight_bytes: usize,
) -> Result<Vec<PreparedDoc>, String>
where
    I: IntoIterator<Item = SourceDocument>;
```

Clamp concurrency to `1..=32`. Use `futures::stream::iter(docs).map(...).buffer_unordered(concurrency)` so code chunking can run concurrently without unbounded `spawn_blocking`.

- [ ] **Step 2: Add byte-budget helper**

Add:

```rust
pub(crate) fn should_flush_prepared_batch(
    docs: &[PreparedDoc],
    next_estimated_bytes: usize,
    max_docs: usize,
    max_bytes: usize,
) -> bool
```

Use it in local, git, scrape batch embedding, and sessions where batching exists. Do not change YouTube playlist sequencing around `yt-dlp`; only avoid retaining duplicate raw text after each video is prepared.

- [ ] **Step 3: Add performance tests**

Add tests proving:

- `prepare_source_documents_bounded` honors the concurrency clamp.
- batch flushing triggers by byte size even when doc count is low.
- Reddit still uses a bounded drain.
- vertical structured payload over the cap is dropped before per-chunk multiplication.

- [ ] **Step 4: Run source-doc performance tests**

Run:

```bash
cargo test -p axon --lib vector::ops::source_doc
```

Expected: pass.

## Task 8: Constructor/Import Guard And Documentation

**Files:**
- Create: `src/vector/ops/source_doc_audit_tests.rs`
- Modify: `src/vector/ops.rs`
- Modify: `src/ingest/CLAUDE.md`
- Modify: `src/vector/CLAUDE.md`

- [ ] **Step 1: Add narrow audit tests**

Add an audit test that scans only production source files under `src/ingest`, `src/services/scrape.rs`, and `src/vector/ops/tei/prepare.rs` for forbidden imports and manual constructors:

- `use crate::vector::ops::{chunk_text`
- `use crate::vector::ops::{chunk_markdown`
- `use crate::vector::ops::{chunk_file`
- `PreparedDoc {`
- `PreparedDoc::ingest(`
- `PreparedDoc::from_planned_chunks(`

Allowlist only:

- `src/vector/ops/source_doc.rs`
- `src/vector/ops/tei.rs`
- low-level chunker tests

This guard is a backstop. The real enforcement is constructor privacy and behavior tests.

- [ ] **Step 2: Update repo docs**

In `src/ingest/CLAUDE.md`, replace guidance that says ingestion builders chunk before `PreparedDoc` with:

```text
Ingest builders produce SourceDocument values or call source_doc helpers.
Only the source_doc planner calls chunk_file/chunk_markdown/chunk_text.
PreparedDoc is post-chunk and embed-ready.
```

In `src/vector/CLAUDE.md`, document payload schema v8 and optionality of `content_kind`, `chunk_locator`, and `source_range` on mixed collections.

- [ ] **Step 3: Run audit/doc-adjacent tests**

Run:

```bash
cargo test -p axon --lib source_doc_audit
```

Expected: pass.

## Task 9: Save Bead And Final Verification

**Files:**
- Plan already saved: `docs/superpowers/plans/2026-06-13-normalized-pre-chunk-documents.md`

- [ ] **Step 1: Create a bead before implementation**

Run:

```bash
bd create --title="Normalize pre-chunk document pipeline" --description="Add SourceDocument planner with typed metadata, bounded planning, vertical structured preservation, and route local/git/scrape/plain ingest through it." --type=task --priority=2
```

Expected: a new `axon_rust-*` task id.

- [ ] **Step 2: Format**

Run:

```bash
cargo fmt --all
```

Expected: no formatting errors.

- [ ] **Step 3: Run focused tests**

Run:

```bash
cargo test -p axon --lib vector::ops::source_doc
cargo test -p axon --lib vector::ops::tei::pipeline_tests
cargo test -p axon --lib vector::ops::tei::prepare_tests
cargo test -p axon --lib services::scrape_tests
cargo test -p axon --lib ingest::github ingest::gitlab ingest::generic_git
cargo test -p axon --lib ingest::reddit ingest::youtube ingest::github::issues ingest::gitea ingest::sessions
cargo test -p axon --lib source_doc_audit
```

Expected: all pass.

- [ ] **Step 4: Run full library tests**

Run:

```bash
cargo test -p axon --lib
```

Expected: all pass.

- [ ] **Step 5: Run workspace check**

Run:

```bash
cargo check --workspace --all-targets
```

Expected: pass.

- [ ] **Step 6: Inspect changed files**

Run:

```bash
git status --short
git diff --stat
```

Expected: only the normalized pre-chunk document implementation, tests, plan doc, docs updates, and any bead metadata changed by this work are included. Pre-existing unrelated deletions under `docs/palette-demo/` and unrelated docs reports must not be staged or reverted.

## Review Completeness Checklist

- Architecture P1: Existing `code_*`/`git_*` metadata parity is explicitly tested and preserved.
- Architecture P1: Source origin prevents crawl manifest `.rs` URLs from being code-chunked.
- Architecture P1/Simplicity Medium: Existing `apply_extra` is reused; no duplicate merge helper is added.
- Architecture P2: Vertical output format semantics are tested separately from embedding markdown.
- Simplicity High: Control-character markdown fallback is preserved.
- Simplicity Medium: File-path chunking uses `chunk_file` only; no duplicate markdown extension helper exists.
- Simplicity Medium: Plain-text providers avoid unnecessary async rewrites.
- Security High: Planner-owned normalized fields cannot be injected through raw caller `extra`.
- Security High: Local path locators are relative/redacted and old local fragment points are cleaned.
- Security Medium: Public structured payloads are redacted/capped before serialization.
- Security Medium: URL/domain/source construction is centralized and validated.
- Security Medium: Payload schema is bumped and mixed old/new compatibility is documented.
- Performance P1: Planning is bounded and can run code chunking concurrently.
- Performance P1: Batch flushing considers bytes, not just doc count.
- Performance P1: Reddit streaming/backpressure shape is preserved.
- Performance P2: GitHub wiki and top-level GitHub docs are in scope.
