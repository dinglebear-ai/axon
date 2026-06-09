use super::*;

fn test_ctx(repo_root: PathBuf) -> FileEmbedCtx {
    FileEmbedCtx {
        cfg: Config::test_default(),
        repo_root,
        owner: "owner".into(),
        name: "repo".into(),
        default_branch: "main".into(),
        repo_description: None,
        pushed_at: None,
        is_private: Some(false),
    }
}

#[tokio::test]
async fn read_file_embed_docs_writes_symbol_payload_contract() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    tokio::fs::create_dir_all(&src_dir)
        .await
        .expect("create src");
    tokio::fs::write(
        src_dir.join("lib.rs"),
        format!(
            "struct Response;\n\nimpl Response {{\n    pub fn parse(&self) {{\n{}\n    }}\n}}\n",
            (0..90)
                .map(|i| format!("        let value_{i} = {i};"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    )
    .await
    .expect("write rust file");

    let docs = read_file_embed_docs(&test_ctx(tmp.path().to_path_buf()), "src/lib.rs")
        .await
        .expect("read docs");

    let method_doc = docs
        .iter()
        .find(|doc| {
            doc.extra
                .as_ref()
                .and_then(|extra| extra.get("symbol_name"))
                .and_then(|value| value.as_str())
                == Some("Response::parse")
        })
        .expect("method doc with symbol payload");
    let extra = method_doc.extra.as_ref().expect("github payload");
    assert_eq!(extra["provider"], "github");
    assert_eq!(extra["git_content_kind"], "file");
    assert_eq!(extra["code_chunking_method"], "tree_sitter");
    assert_eq!(extra["symbol_name"], "Response::parse");
    assert_eq!(extra["symbol_kind"], "method");
    assert_eq!(extra["symbol_extraction_status"], "ok");
    assert!(
        method_doc
            .url
            .starts_with("https://github.com/owner/repo/blob/main/src/lib.rs#L")
    );
}

#[tokio::test]
async fn read_file_embed_docs_reports_unexpected_read_failures() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let err = read_file_embed_docs(&test_ctx(tmp.path().to_path_buf()), "missing.rs")
        .await
        .expect_err("missing file should be a read failure");
    assert!(err.contains("stat failed for missing.rs"));
}

#[tokio::test]
async fn non_utf8_file_is_skipped_not_failed() {
    // A non-UTF-8 file must be a benign skip (Ok, empty), not an Err — otherwise a
    // single binary/Latin-1 file would abort the whole repo ingest.
    let tmp = tempfile::tempdir().expect("tempdir");
    tokio::fs::write(tmp.path().join("blob.txt"), [0xff, 0xfe, 0x00, 0x42])
        .await
        .expect("write bytes");
    let docs = read_file_embed_docs(&test_ctx(tmp.path().to_path_buf()), "blob.txt")
        .await
        .expect("non-utf8 file should skip, not error");
    assert!(docs.is_empty());
}

#[test]
fn text_chunks_line_ranges_are_monotonic_with_duplicate_lines() {
    // Repeated identical lines make chunk text ambiguous; the moving-cursor offset
    // resolution must still assign non-regressing, correct line ranges (the
    // behaviour the deleted line_range tests guarded).
    let line = "the quick brown fox jumps over the lazy dog\n";
    let text = line.repeat(200);
    let chunks = text_chunks(&text);
    assert!(chunks.len() >= 2, "expected multiple prose chunks");
    let mut prev_start = 0u32;
    for chunk in &chunks {
        assert!(chunk.start_line <= chunk.end_line);
        assert!(
            chunk.start_line >= prev_start,
            "line ranges must not regress across chunks"
        );
        prev_start = chunk.start_line;
        assert!(chunk.symbol.is_none());
    }
    let last = chunks.last().expect("at least one chunk");
    assert!(
        last.end_line > chunks[0].end_line,
        "a later chunk must cover later lines"
    );
    assert!(last.end_line <= 201);
}

#[test]
fn prose_file_uses_prose_chunking_method() {
    let text = format!("# Title\n\n{}", "prose body line\n".repeat(60));
    let chunks = code_or_text_chunks(&text, "md");
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|chunk| chunk.symbol.is_none()));
    assert_eq!(chunking_method("md", &chunks[0]), "prose");
}
