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
fn chunking_method_returns_prose_for_chunk_without_symbol() {
    use crate::vector::ops::input::code::CodeChunk;
    let chunk = CodeChunk {
        text: "hello".into(),
        byte_start: 0,
        byte_end: 5,
        start_line: 1,
        end_line: 1,
        declaration_start_line: 1,
        declaration_end_line: 1,
        symbol: None,
    };
    // Even for a Rust file extension, a symbol-less chunk should be labeled "prose"
    assert_eq!(chunking_method("rs", &chunk), "prose");
}
