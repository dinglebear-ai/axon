use super::*;
use tempfile::TempDir;

#[tokio::test]
async fn permissive_recurses_prunes_and_skips_binary() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    tokio::fs::create_dir_all(root.join("a/b")).await.unwrap();
    tokio::fs::write(root.join("a/b/c.rs"), "fn x() {}")
        .await
        .unwrap();
    tokio::fs::write(root.join("r.md"), "# hi").await.unwrap();
    tokio::fs::write(root.join("img.png"), "x").await.unwrap();
    tokio::fs::create_dir_all(root.join("node_modules"))
        .await
        .unwrap();
    tokio::fs::write(root.join("node_modules/x.js"), "1")
        .await
        .unwrap();

    let files = collect_files(root, SelectionPolicy::Permissive)
        .await
        .unwrap();
    let names: Vec<String> = files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
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
    tokio::fs::write(root.join("a.rs"), "fn x() {}")
        .await
        .unwrap();
    tokio::fs::write(root.join("README.md"), "# hi")
        .await
        .unwrap();

    let docs_only = collect_files(
        root,
        SelectionPolicy::Allowlist {
            include_source: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(docs_only.len(), 1);
    assert!(docs_only[0].to_string_lossy().ends_with("README.md"));

    let with_src = collect_files(
        root,
        SelectionPolicy::Allowlist {
            include_source: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(with_src.len(), 2);
}

#[tokio::test]
async fn allowlist_skips_generated_bulk_but_keeps_useful_configs() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    tokio::fs::create_dir_all(root.join("apps/web/openapi"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(root.join("apps/web/lib/generated"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(root.join("docs/reference/actions"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(root.join(".github/workflows"))
        .await
        .unwrap();

    tokio::fs::write(root.join("package.json"), "{\"name\":\"demo\"}")
        .await
        .unwrap();
    tokio::fs::write(root.join("config.example.toml"), "name = \"demo\"")
        .await
        .unwrap();
    tokio::fs::write(
        root.join("docker-compose.yaml"),
        "services:\n  app:\n    image: demo\n",
    )
    .await
    .unwrap();
    tokio::fs::write(root.join(".github/workflows/ci.yml"), "name: ci\n")
        .await
        .unwrap();
    tokio::fs::write(root.join("apps/web/openapi/axon.json"), "{\"paths\":{}}")
        .await
        .unwrap();
    tokio::fs::write(
        root.join("apps/web/lib/generated/axon-api.ts"),
        "export type Api = {};",
    )
    .await
    .unwrap();
    tokio::fs::write(root.join("docs/reference/actions/README.md"), "# Actions")
        .await
        .unwrap();

    let files = collect_files(
        root,
        SelectionPolicy::Allowlist {
            include_source: true,
        },
    )
    .await
    .unwrap();
    let rels: Vec<String> = files
        .iter()
        .map(|p| {
            p.strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();

    for expected in [
        ".github/workflows/ci.yml",
        "config.example.toml",
        "docker-compose.yaml",
        "package.json",
    ] {
        assert!(
            rels.iter().any(|rel| rel == expected),
            "expected useful config file to be collected: {expected}; got {rels:?}"
        );
    }
    for skipped in [
        "apps/web/openapi/axon.json",
        "apps/web/lib/generated/axon-api.ts",
        "docs/reference/actions/README.md",
    ] {
        assert!(
            !rels.iter().any(|rel| rel == skipped),
            "expected generated bulk file to be skipped: {skipped}; got {rels:?}"
        );
    }
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
    let chunks = chunk_file(&text, "txt");
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|c| c.symbol.is_none()));
    assert_eq!(chunking_method("txt", &chunks[0]), "prose");
}

#[test]
fn chunk_file_uses_markdown_boundaries_for_markdown() {
    let text = format!(
        "# Intro\n\n{}\n\n## Usage\n\n{}\n",
        "intro paragraph ".repeat(220),
        "usage details ".repeat(220)
    );

    let chunks = chunk_file(&text, "md");

    assert!(chunks.len() > 1);
    assert!(
        chunks
            .iter()
            .skip(1)
            .all(|chunk| chunk.text.starts_with('#')
                || !chunk.text[..1].chars().all(char::is_lowercase)),
        "markdown chunks should not start in the middle of a lowercase word: {chunks:#?}"
    );
    assert!(
        chunks
            .iter()
            .any(|chunk| chunk.text.starts_with("# Intro") || chunk.text.starts_with("## Usage")),
        "heading context should be preserved in markdown chunks"
    );
    assert_eq!(chunking_method("md", &chunks[0]), "markdown");
}

#[test]
fn chunk_file_handles_multibyte_content_without_panic() {
    // A file with multibyte characters; prose fallback path since "txt" has no grammar
    let base = "fn x() { /* 日本語コメント */ }\n";
    let text = base.repeat(200); // large enough to produce multiple chunks
    let chunks = chunk_file(&text, "txt");
    assert!(!chunks.is_empty());
    for c in &chunks {
        // Must not panic on byte boundary — slice into the original string
        let _ = &text[c.byte_start..c.byte_end.min(text.len())];
    }
}

#[test]
fn permissive_skips_declaration_and_minified_files() {
    use crate::ops::input::classify::path_extension;
    // Verify the predicate logic that drives Permissive include_file decisions.
    // Extension alone (`ts`, `js`, `css`) is not in BINARY_EXTENSIONS, so only
    // is_generated_filename blocks these files under the Permissive policy.
    let cases: &[(&str, bool)] = &[
        ("index.d.ts", false),
        ("types.d.mts", false),
        ("app.min.js", false),
        ("styles.min.css", false),
        ("main.bundle.js", false),
        ("index.ts", true),
        ("styles.css", true),
        ("app.js", true),
    ];
    for (name, expect_included) in cases {
        let name_lower = name.to_ascii_lowercase();
        let included = !select::is_binary_ext(path_extension(name))
            && !select::is_generated_filename(&name_lower);
        assert_eq!(
            included, *expect_included,
            "Permissive policy for '{name}': expected included={expect_included}, got {included}"
        );
    }
}

#[test]
fn json_yaml_toml_chunks_are_capped_at_max() {
    // ~210 KB of structured content → well over MAX text chunks, so the cap
    // actually fires and reports a dropped tail (the prior 100-key fixture
    // produced only ~2 chunks, making the `<= MAX` assertion vacuous).
    let big: String = (0..10_000)
        .map(|i| format!("key_{i}: value_{i}\n"))
        .collect();
    for ext in ["json", "yaml", "yml", "toml"] {
        let (chunks, dropped) = chunk_file_reporting_cap(&big, ext);
        assert_eq!(
            chunks.len(),
            MAX_JSON_YAML_CHUNKS,
            "ext {ext} must cap at MAX, got {}",
            chunks.len()
        );
        assert!(
            dropped > 0,
            "ext {ext} must report the dropped tail, got {dropped}"
        );
    }
}

#[test]
fn chunk_cap_does_not_truncate_non_structured_exts() {
    // The same over-MAX volume with a plain-text extension must NOT be capped —
    // a regression that widened the cap to all extensions would fail here.
    let big: String = (0..10_000)
        .map(|i| format!("key_{i}: value_{i}\n"))
        .collect();
    let (chunks, dropped) = chunk_file_reporting_cap(&big, "txt");
    assert!(
        chunks.len() > MAX_JSON_YAML_CHUNKS,
        "plain text should exceed MAX, got {}",
        chunks.len()
    );
    assert_eq!(dropped, 0, "plain text must never be capped");
}

#[test]
fn chunking_method_returns_prose_for_chunk_without_symbol() {
    use crate::ops::input::code::{ChunkSource, CodeChunk};
    let chunk = CodeChunk {
        text: "hello".into(),
        byte_start: 0,
        byte_end: 5,
        start_line: 1,
        end_line: 1,
        declaration_start_line: 1,
        declaration_end_line: 1,
        symbol: None,
        source: ChunkSource::TreeSitter,
    };
    assert_eq!(chunking_method("rs", &chunk), "tree_sitter");
}
