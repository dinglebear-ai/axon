use super::prepare_embed_docs;
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

    // Nested source file (should be embedded).
    tokio::fs::create_dir_all(root.join("a/b"))
        .await
        .expect("mkdir a/b");
    tokio::fs::write(root.join("a/b/c.rs"), "fn main() { println!(\"hi\"); }")
        .await
        .expect("write c.rs");
    // Top-level doc (should be embedded).
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
    assert_eq!(
        prepared.len(),
        2,
        "expected only c.rs and r.md, got {urls:?}"
    );
    assert!(urls.iter().any(|u| u.ends_with("a/b/c.rs")), "{urls:?}");
    assert!(urls.iter().any(|u| u.ends_with("r.md")), "{urls:?}");
    assert!(!urls.iter().any(|u| u.ends_with("img.png")), "{urls:?}");
    assert!(!urls.iter().any(|u| u.contains("node_modules")), "{urls:?}");
}

/// Code files route through AST chunking and are tagged `content_type = "text"`;
/// markdown/docs stay on the prose path tagged `"markdown"`.
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
        .find(|d| d.url.ends_with("lib.rs"))
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
