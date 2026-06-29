use super::*;

#[tokio::test]
async fn generic_file_docs_chunk_rust_as_code_with_symbols() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("lib.rs"), "fn alpha() {}\n\nfn beta() {}\n").unwrap();
    let target = GenericGitTarget {
        host: "example.com".into(),
        name: "repo".into(),
        clone_url: "https://example.com/r.git".into(),
        web_url: "https://example.com/r".into(),
    };
    let docs = file_docs(root, &target, "main", root.join("lib.rs"), "git", "git")
        .await
        .unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].chunks().len(), docs[0].chunk_extra().len());
    let extra = docs[0].extra().unwrap();
    assert_eq!(extra["code_file_type"], "source");
    assert_eq!(extra["code_file_path"], "lib.rs");
    assert!(
        docs[0]
            .chunk_extra()
            .iter()
            .any(|extra| extra["code_chunking_method"] == "tree_sitter"),
        "expected at least one tree-sitter chunk"
    );
    assert!(
        docs[0]
            .chunk_extra()
            .iter()
            .any(|extra| extra["symbol_kind"] == "function"),
        "expected at least one function-symbol chunk"
    );
}

#[tokio::test]
async fn generic_file_docs_mark_markdown_and_text_chunks_by_actual_chunker() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("README.md"), "# Readme\n\ntext").unwrap();
    std::fs::write(root.join("notes.txt"), "plain notes").unwrap();
    let target = GenericGitTarget {
        host: "example.com".into(),
        name: "repo".into(),
        clone_url: "https://example.com/r.git".into(),
        web_url: "https://example.com/r".into(),
    };

    let md_docs = file_docs(root, &target, "main", root.join("README.md"), "git", "git")
        .await
        .unwrap();
    let txt_docs = file_docs(root, &target, "main", root.join("notes.txt"), "git", "git")
        .await
        .unwrap();

    assert_eq!(
        md_docs[0].chunk_extra()[0]["chunk_content_kind"],
        "markdown"
    );
    assert_eq!(
        txt_docs[0].chunk_extra()[0]["chunk_content_kind"],
        "plain_text"
    );
}

#[test]
fn parses_explicit_https_git_target() {
    let target = parse_generic_git_target("git:https://example.com/org/repo.git").unwrap();
    assert_eq!(target.host, "example.com");
    assert_eq!(target.name, "repo");
    assert_eq!(target.clone_url, "https://example.com/org/repo.git");
    assert_eq!(target.web_url, "https://example.com/org/repo");
}

#[test]
fn rejects_non_https_generic_git_target() {
    assert!(parse_generic_git_target("git:ssh://example.com/org/repo.git").is_err());
    assert!(parse_generic_git_target("git:http://example.com/org/repo.git").is_err());
}

#[tokio::test]
async fn git_branch_remove_creates_cleanup_debt_without_qdrant_scroll() {
    let pool = open_source_ledger_pool(":memory:").await.unwrap();
    let store = SourceLedgerStore::new(pool);
    let cfg = Config::default();
    let target = GenericGitTarget {
        host: "example.com".into(),
        name: "repo".into(),
        clone_url: "https://example.com/org/repo.git".into(),
        web_url: "https://example.com/org/repo".into(),
    };
    let source = git_source_identity(&cfg, &target, "main");

    store.ensure_source(&source).await.unwrap();
    store
        .record_manifest_item(
            &source.source_id,
            1,
            ManifestItem::new("src/removed.rs", "old-hash", 20),
        )
        .await
        .unwrap();
    store.commit_generation(&source.source_id, 1).await.unwrap();

    let generation = commit_git_manifest_with_store(
        &store,
        &source,
        &target,
        "main",
        &[ManifestItem::new("src/lib.rs", "new-hash", 30)],
    )
    .await
    .unwrap();

    assert_eq!(generation, 2);
    assert_eq!(
        store.cleanup_debt_count(&source.source_id).await.unwrap(),
        1
    );
}

#[test]
fn immutable_commit_sha_does_not_schedule_refresh() {
    assert!(!git_ref_schedules_refresh(
        "0123456789abcdef0123456789abcdef01234567"
    ));
    assert!(git_ref_schedules_refresh("main"));
}

#[tokio::test]
async fn file_docs_returns_empty_for_whitespace_only_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("empty.rs"), "   \n\n   \n").unwrap();
    let target = GenericGitTarget {
        host: "example.com".into(),
        name: "repo".into(),
        clone_url: "https://example.com/r.git".into(),
        web_url: "https://example.com/r".into(),
    };
    let docs = file_docs(root, &target, "main", root.join("empty.rs"), "git", "git")
        .await
        .unwrap();
    assert!(
        docs.is_empty(),
        "whitespace-only files should produce no PreparedDocs"
    );
}

#[tokio::test]
async fn file_docs_skips_non_utf8_without_error() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("binary.rs"), b"\xff\xfe invalid utf8 \x00").unwrap();
    let target = GenericGitTarget {
        host: "example.com".into(),
        name: "repo".into(),
        clone_url: "https://example.com/r.git".into(),
        web_url: "https://example.com/r".into(),
    };
    let result = file_docs(root, &target, "main", root.join("binary.rs"), "git", "git").await;
    assert!(
        result.is_ok(),
        "non-UTF-8 file should not propagate an error"
    );
    assert!(
        result.unwrap().is_empty(),
        "non-UTF-8 file should produce no PreparedDocs"
    );
}
