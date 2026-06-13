use super::{prepare_embed_docs, read_inputs, read_inputs_with_max_bytes};
use crate::core::config::Config;
use tempfile::TempDir;

#[tokio::test]
async fn prepare_embed_docs_uses_given_source_type() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let input_path = temp_dir.path().join("doc.md");
    tokio::fs::write(&input_path, "# Crawl doc\n\nhello there")
        .await
        .expect("write markdown fixture");

    let prepared = prepare_embed_docs(&cfg, &input_path.to_string_lossy(), &[], Some("crawl"))
        .await
        .expect("prepare docs");

    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].source_type, "crawl");
}

#[tokio::test]
async fn prepare_embed_docs_defaults_to_embed() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let input_path = temp_dir.path().join("doc.md");
    tokio::fs::write(&input_path, "# Embed doc\n\nthis is a test")
        .await
        .expect("write markdown fixture");

    let prepared = prepare_embed_docs(&cfg, &input_path.to_string_lossy(), &[], None)
        .await
        .expect("prepare docs");

    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].source_type, "embed");
}

/// A directory embed recurses into subdirectories, prunes junk dirs, and skips
/// binary-extension files.
#[tokio::test]
async fn dir_embed_recurses_and_filters() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let root = temp_dir.path();

    // Nested source file (should be embedded — one PreparedDoc per chunk).
    tokio::fs::create_dir_all(root.join("a/b"))
        .await
        .expect("mkdir a/b");
    tokio::fs::write(root.join("a/b/c.rs"), "fn main() { println!(\"hi\"); }")
        .await
        .expect("write c.rs");
    // Top-level doc (should be embedded — one PreparedDoc).
    tokio::fs::write(root.join("r.md"), "# Title\n\nbody text here")
        .await
        .expect("write r.md");
    // Binary-extension file (should be skipped by extension filter).
    tokio::fs::write(root.join("img.png"), "not really a png")
        .await
        .expect("write img.png");
    // Pruned directory contents (should never be descended into).
    tokio::fs::create_dir_all(root.join("node_modules"))
        .await
        .expect("mkdir node_modules");
    tokio::fs::write(root.join("node_modules/x.js"), "console.log(1)")
        .await
        .expect("write node_modules/x.js");

    let prepared = prepare_embed_docs(&cfg, &root.to_string_lossy(), &[], None)
        .await
        .expect("prepare docs");

    let urls: Vec<&str> = prepared.iter().map(|d| d.url.as_str()).collect();
    // c.rs is a code file → one PreparedDoc per chunk (at least 1); r.md is prose → 1 doc.
    assert!(
        prepared.len() >= 2,
        "expected at least one doc for c.rs and one for r.md, got {urls:?}"
    );
    // Code chunks carry `#L{start}-L{end}` suffix; the path prefix is still present.
    assert!(
        urls.iter().any(|u| u.contains("a/b/c.rs")),
        "expected a/b/c.rs in urls: {urls:?}"
    );
    assert!(urls.iter().any(|u| u.ends_with("r.md")), "{urls:?}");
    assert!(!urls.iter().any(|u| u.ends_with("img.png")), "{urls:?}");
    assert!(!urls.iter().any(|u| u.contains("node_modules")), "{urls:?}");
}

/// Code files route through AST chunking and are tagged `content_type = "text"`;
/// markdown/docs stay on the prose path tagged `"markdown"`. Code files produce
/// one PreparedDoc per chunk, with the chunk URL carrying a `#L{start}-L{end}` suffix.
#[tokio::test]
async fn dir_embed_tags_code_and_prose_distinctly() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let root = temp_dir.path();
    tokio::fs::write(root.join("lib.rs"), "fn a() {}\n\nfn b() {}\n")
        .await
        .expect("write lib.rs");
    tokio::fs::write(root.join("readme.md"), "# Readme\n\nprose content")
        .await
        .expect("write readme.md");

    let prepared = prepare_embed_docs(&cfg, &root.to_string_lossy(), &[], None)
        .await
        .expect("prepare docs");

    let rs = prepared
        .iter()
        .find(|d| d.url.contains("lib.rs"))
        .expect("lib.rs doc");
    let md = prepared
        .iter()
        .find(|d| d.url.ends_with("readme.md"))
        .expect("readme.md doc");
    assert_eq!(rs.content_type, "text", "code should be tagged text");
    assert_eq!(
        md.content_type, "markdown",
        "docs should be tagged markdown"
    );
    assert!(!rs.chunks.is_empty());
    assert_eq!(rs.chunks.len(), rs.chunk_extra.len());
    assert_eq!(rs.chunk_extra[0]["content_kind"], "code");
    assert!(
        rs.chunk_extra[0]["chunk_locator"]
            .as_str()
            .unwrap()
            .contains("lib.rs#L")
    );
    assert!(rs.chunk_extra[0]["code_line_start"].as_u64().is_some());
    let extra = rs
        .extra
        .as_ref()
        .expect("code file must have extra payload");
    assert!(extra.get("code_file_type").is_some());
}

#[tokio::test]
async fn crawl_manifest_rs_url_stays_markdown_not_code() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let root = temp_dir.path();
    let markdown_dir = root.join("markdown");
    tokio::fs::create_dir_all(&markdown_dir)
        .await
        .expect("mkdir markdown");
    let file = markdown_dir.join("lib.md");
    tokio::fs::write(&file, "fn looks_like_code() {}\n")
        .await
        .expect("write file");
    let canonical = std::fs::canonicalize(&file).expect("canonical");
    tokio::fs::write(
        root.join("manifest.jsonl"),
        format!(
            "{}\n",
            serde_json::json!({
                "url": "https://example.com/src/lib.rs",
                "file_path": canonical.to_string_lossy(),
                "changed": true
            })
        ),
    )
    .await
    .expect("write manifest");

    let prepared = prepare_embed_docs(&cfg, &markdown_dir.to_string_lossy(), &[], Some("crawl"))
        .await
        .expect("prepare docs");

    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].content_type, "markdown");
    assert_eq!(prepared[0].chunk_extra[0]["content_kind"], "markdown");
    assert!(prepared[0].chunk_extra[0].get("code_line_start").is_none());
}

#[tokio::test]
async fn crawl_manifest_markdown_with_control_chars_does_not_panic() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let root = temp_dir.path();
    let markdown_dir = root.join("markdown");
    tokio::fs::create_dir_all(&markdown_dir)
        .await
        .expect("mkdir markdown");
    let file = markdown_dir.join("control.md");
    tokio::fs::write(&file, "# Title\n\nbad\u{0008}content")
        .await
        .expect("write file");
    let canonical = std::fs::canonicalize(&file).expect("canonical");
    tokio::fs::write(
        root.join("manifest.jsonl"),
        format!(
            "{}\n",
            serde_json::json!({
                "url": "https://example.com/control",
                "file_path": canonical.to_string_lossy(),
                "changed": true
            })
        ),
    )
    .await
    .expect("write manifest");

    let prepared = prepare_embed_docs(&cfg, &markdown_dir.to_string_lossy(), &[], Some("crawl"))
        .await
        .expect("prepare docs");

    assert_eq!(prepared.len(), 1);
    assert_eq!(
        prepared[0].chunk_extra[0]["chunking_fallback"],
        "plain_text_control_chars"
    );
}

/// A single unreadable / non-UTF-8 file in the directory is skipped, not fatal —
/// the rest of the directory still embeds.
#[tokio::test]
async fn dir_embed_skips_unreadable_file_without_failing() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let root = temp_dir.path();
    // ".dat" is not on the binary-extension denylist, so it reaches read_to_string
    // and fails the UTF-8 decode — exercising the skip-on-error path.
    tokio::fs::write(root.join("blob.dat"), [0xff, 0xff, 0xfe])
        .await
        .expect("write blob.dat");
    tokio::fs::write(root.join("ok.md"), "# Ok\n\nreadable content")
        .await
        .expect("write ok.md");

    let prepared = prepare_embed_docs(&cfg, &root.to_string_lossy(), &[], None)
        .await
        .expect("prepare docs should not fail on one bad file");

    assert_eq!(prepared.len(), 1);
    assert!(prepared[0].url.ends_with("ok.md"));
}

/// Crawl-output regression: a directory carrying a `manifest.jsonl` must still
/// honor the `changed == false` skip, reconstruct the structured payload, and
/// chunk markdown as prose (not code).
#[tokio::test]
async fn dir_embed_honors_crawl_manifest() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let root = temp_dir.path();
    let markdown_dir = root.join("markdown");
    tokio::fs::create_dir_all(&markdown_dir)
        .await
        .expect("mkdir markdown");

    let unchanged = markdown_dir.join("001-old.md");
    let changed = markdown_dir.join("002-new.md");
    tokio::fs::write(&unchanged, "# Old\n\nshould be skipped")
        .await
        .expect("write unchanged");
    tokio::fs::write(&changed, "# New\n\nshould be embedded")
        .await
        .expect("write changed");

    let unchanged_canon = std::fs::canonicalize(&unchanged).expect("canon unchanged");
    let changed_canon = std::fs::canonicalize(&changed).expect("canon changed");
    let manifest = root.join("manifest.jsonl");
    let line_unchanged = serde_json::json!({
        "url": "https://example.com/old",
        "file_path": unchanged_canon.to_string_lossy(),
        "changed": false,
    });
    let line_changed = serde_json::json!({
        "url": "https://example.com/new",
        "file_path": changed_canon.to_string_lossy(),
        "changed": true,
        "structured": { "kind": "jsonld", "blob": { "@type": "Article" }, "schema_type": "Article" },
    });
    tokio::fs::write(&manifest, format!("{line_unchanged}\n{line_changed}\n"))
        .await
        .expect("write manifest");

    let prepared = prepare_embed_docs(&cfg, &markdown_dir.to_string_lossy(), &[], Some("crawl"))
        .await
        .expect("prepare docs");

    // changed==false file skipped; only the changed file remains.
    assert_eq!(prepared.len(), 1, "unchanged file should be skipped");
    let doc = &prepared[0];
    assert_eq!(doc.url, "https://example.com/new");
    // Manifest URL → http source → prose chunking, not code.
    assert_eq!(doc.content_type, "markdown");
    // Structured payload reconstructed from the manifest blob.
    let structured = doc.structured.as_ref().expect("structured payload present");
    assert_eq!(structured.kind, "jsonld");
    assert_eq!(structured.schema_type.as_deref(), Some("Article"));
}

/// The reader skips symlinked entries (their `file_type` is neither file nor
/// dir), so a symlink-to-file is never embedded. Guards against a regression
/// where the file-type dispatch is "simplified" into following symlinks.
#[tokio::test]
#[cfg(unix)]
async fn dir_embed_skips_symlinks() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let root = temp_dir.path();
    tokio::fs::write(root.join("real.md"), "# Real\n\nembedded content")
        .await
        .expect("write real.md");
    std::os::unix::fs::symlink(root.join("real.md"), root.join("link.md")).expect("symlink");

    let prepared = prepare_embed_docs(&cfg, &root.to_string_lossy(), &[], None)
        .await
        .expect("prepare docs");

    let urls: Vec<&str> = prepared.iter().map(|d| d.url.as_str()).collect();
    assert!(urls.iter().any(|u| u.ends_with("real.md")), "{urls:?}");
    assert!(!urls.iter().any(|u| u.ends_with("link.md")), "{urls:?}");
}

/// POSIX-style symlink policy: a symlink named explicitly as the embed target
/// IS followed (like `du`/`find -H` follow command-line symlinks), while
/// symlinks encountered during traversal are skipped (covered above). Guards
/// the intent so the root-follow isn't "fixed" into a skip — or vice versa —
/// without revisiting the policy.
#[tokio::test]
#[cfg(unix)]
async fn dir_embed_follows_explicit_root_symlink() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let real_root = temp_dir.path().join("real");
    tokio::fs::create_dir_all(&real_root)
        .await
        .expect("real dir");
    tokio::fs::write(real_root.join("doc.md"), "# Doc\n\nlinked-root content")
        .await
        .expect("write doc.md");
    let link_root = temp_dir.path().join("link");
    std::os::unix::fs::symlink(&real_root, &link_root).expect("root symlink");

    let prepared = prepare_embed_docs(&cfg, &link_root.to_string_lossy(), &[], None)
        .await
        .expect("prepare docs");

    assert_eq!(prepared.len(), 1, "explicit root symlink must be followed");
    assert!(prepared[0].url.ends_with("doc.md"));
}

/// An empty/whitespace-only code file chunks to zero chunks and is skipped — it
/// must not produce a zero-chunk PreparedDoc.
#[tokio::test]
async fn dir_embed_skips_empty_code_file() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let root = temp_dir.path();
    tokio::fs::write(root.join("empty.rs"), "   \n\n")
        .await
        .expect("write empty.rs");
    tokio::fs::write(root.join("ok.md"), "# Ok\n\nreadable content")
        .await
        .expect("write ok.md");

    let prepared = prepare_embed_docs(&cfg, &root.to_string_lossy(), &[], None)
        .await
        .expect("prepare docs");

    assert_eq!(prepared.len(), 1, "empty code file should be skipped");
    assert!(prepared[0].url.ends_with("ok.md"));
}

/// A path-shaped input that doesn't resolve must be a hard error — never
/// embedded as free text. Guards the container-claims-host-path failure where
/// `/home/<user>/docs` was "successfully" embedded as a one-chunk document by
/// a worker that couldn't see the path.
#[tokio::test]
async fn missing_path_like_input_errors_instead_of_embedding_the_string() {
    let err = read_inputs("/definitely/not/a/real/path/docs")
        .await
        .expect_err("missing path-like input must error");
    assert!(
        err.to_string().contains("does not exist or is not visible"),
        "{err}"
    );
}

/// Free-text input (not path-shaped) still embeds as a text document.
#[tokio::test]
async fn free_text_input_still_embeds_as_text() {
    let records = read_inputs("just some words to embed")
        .await
        .expect("free text embeds");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].1, "just some words to embed");
}

/// Directory walks skip oversized files with a warning; an explicitly named
/// oversized file is a hard error so the user learns the cap.
#[tokio::test]
async fn oversized_files_are_skipped_in_dirs_and_rejected_when_explicit() {
    let temp_dir = TempDir::new().expect("tempdir");
    let root = temp_dir.path();
    tokio::fs::write(root.join("big.md"), "x".repeat(64))
        .await
        .expect("write big.md");
    tokio::fs::write(root.join("ok.md"), "# Ok\n\ncontent")
        .await
        .expect("write ok.md");

    // Dir walk with a 32-byte cap: big.md skipped, ok.md read.
    let records = read_inputs_with_max_bytes(&root.to_string_lossy(), 32)
        .await
        .expect("dir embed survives oversized file");
    assert_eq!(records.len(), 1, "oversized file must be skipped");
    assert!(records[0].0.ends_with("ok.md"));

    // Explicit single oversized file: hard error naming the cap.
    let err = read_inputs_with_max_bytes(&root.join("big.md").to_string_lossy(), 32)
        .await
        .expect_err("explicit oversized file must error");
    assert!(err.to_string().contains("local file cap"), "{err}");
}

/// Crawl-output reuse: a manifest entry whose URL carries a code extension must
/// still chunk as prose (http source → prose path), never route to the AST
/// chunker. Directly exercises the `!url.starts_with("http")` guard in
/// `select_chunks`.
#[tokio::test]
async fn dir_embed_http_code_extension_stays_prose() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let root = temp_dir.path();
    let markdown_dir = root.join("markdown");
    tokio::fs::create_dir_all(&markdown_dir)
        .await
        .expect("mkdir markdown");

    let file = markdown_dir.join("001-script.md");
    tokio::fs::write(&file, "# Script\n\nfn main() {}\n")
        .await
        .expect("write file");
    let canon = std::fs::canonicalize(&file).expect("canon");
    let manifest = root.join("manifest.jsonl");
    // The manifest URL deliberately ends in a code extension (.py).
    let line = serde_json::json!({
        "url": "https://example.com/script.py",
        "file_path": canon.to_string_lossy(),
        "changed": true,
    });
    tokio::fs::write(&manifest, format!("{line}\n"))
        .await
        .expect("write manifest");

    let prepared = prepare_embed_docs(&cfg, &markdown_dir.to_string_lossy(), &[], Some("crawl"))
        .await
        .expect("prepare docs");

    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].url, "https://example.com/script.py");
    // http source → prose path despite the .py extension.
    assert_eq!(prepared[0].content_type, "markdown");
}

/// A local Rust code file embedded via `embed <dir>` must produce per-chunk
/// `PreparedDoc`s carrying canonical `code_*` and `symbol_*` extra payload.
/// Exercises the new code branch that mirrors the ingest path so embed and ingest
/// produce equivalent Qdrant payloads for code files.
#[tokio::test]
async fn dir_embed_code_file_gets_symbol_payload() {
    let cfg = Config::default_minimal();
    let temp_dir = TempDir::new().expect("tempdir");
    let root = temp_dir.path();
    tokio::fs::write(
        root.join("lib.rs"),
        "fn alpha() -> u32 { 1 }\n\nfn beta() -> u32 { 2 }\n",
    )
    .await
    .expect("write lib.rs");

    let prepared = prepare_embed_docs(&cfg, &root.to_string_lossy(), &[], None)
        .await
        .expect("prepare docs");

    assert_eq!(prepared.len(), 1, "expected one file-level doc");
    let doc = &prepared[0];
    assert!(
        doc.url.contains("lib.rs"),
        "url should reference lib.rs: {}",
        doc.url
    );
    assert_eq!(
        doc.content_type, "text",
        "code chunks must be tagged 'text'"
    );
    assert_eq!(doc.chunks.len(), doc.chunk_extra.len());
    let extra = doc
        .extra
        .as_ref()
        .expect("code file must have extra payload");
    assert_eq!(
        extra["code_file_type"], "source",
        "lib.rs should be classified as source"
    );
    assert!(extra.get("symbol_extraction_status").is_some());
    assert!(
        doc.chunk_extra
            .iter()
            .any(|extra| extra["code_chunking_method"] == "tree_sitter"),
        "Rust file must use tree-sitter chunking"
    );
    // At least one chunk should carry a function symbol.
    assert!(
        doc.chunk_extra
            .iter()
            .any(|extra| extra["symbol_kind"].as_str() == Some("function")),
        "expected at least one function-symbol chunk"
    );
}
