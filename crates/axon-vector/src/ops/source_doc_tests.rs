use super::{
    SourceDocument, SourceOrigin, prepare_source_document, structured_payload_from_vertical_summary,
};
use crate::ops::input::chunk_markdown_with_offsets;
use crate::ops::tei::StructuredPayload;

#[test]
fn source_document_rejects_spoofed_ledger_extra() {
    let err = SourceDocument::try_new_file(
        SourceOrigin::LocalFile,
        "file:///safe/src/lib.rs".to_string(),
        "src/lib.rs".to_string(),
        "rs".to_string(),
        "fn safe() {}\n".to_string(),
        "local_code",
        Some("src/lib.rs".to_string()),
        Some(serde_json::json!({
            "source_id": "evil"
        })),
    )
    .unwrap_err();
    assert!(err.contains("ledger-owned payload key"));
}

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
    assert_eq!(prepared.chunk_extra[0]["chunk_content_kind"], "markdown");
    assert_eq!(
        prepared.chunk_extra[0]["chunking_fallback"],
        "plain_text_control_chars"
    );
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
    assert_eq!(prepared.chunk_extra[0]["chunk_content_kind"], "markdown");
    assert!(prepared.chunk_extra[0].get("code_line_start").is_none());
}

#[tokio::test]
async fn markdown_source_uses_splitter_offsets_for_heading_context_chunks() {
    let body = "Follow progress → axon crawl status → embedding crawl output. ".repeat(120);
    let text = format!("# Claude Code\n\n## Crawl debugging\n\n{body}\n\n## Done\n\nfinished");
    let expected_ranges = chunk_markdown_with_offsets(&text)
        .into_iter()
        .map(|(start, end, _)| (start, end))
        .collect::<Vec<_>>();
    assert!(
        expected_ranges.len() > 1,
        "fixture must produce multiple markdown chunks"
    );

    let source = SourceDocument::try_new_crawl_manifest(
        "https://code.claude.com/".to_string(),
        text.clone(),
        None,
        None,
    )
    .expect("source doc");

    let prepared = prepare_source_document(source).await.expect("prepared doc");
    let actual_ranges = prepared
        .chunk_extra
        .iter()
        .map(|extra| {
            let range = &extra["source_range"];
            (
                range["byte_start"].as_u64().expect("byte_start") as usize,
                range["byte_end"].as_u64().expect("byte_end") as usize,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(actual_ranges, expected_ranges);
    for (start, end) in actual_ranges {
        assert!(
            text.is_char_boundary(start),
            "start {start} must be a char boundary"
        );
        assert!(
            text.is_char_boundary(end),
            "end {end} must be a char boundary"
        );
    }
}

#[tokio::test]
async fn file_source_attaches_existing_code_keys_and_new_locator_keys() {
    let source = SourceDocument::try_new_file(
        SourceOrigin::GitFile,
        "https://github.com/owner/repo/blob/main/src/lib.rs".to_string(),
        "src/lib.rs".to_string(),
        "rs".to_string(),
        format!(
            "struct Response;\n\nimpl Response {{\n    pub fn parse(&self) {{\n{}\n    }}\n}}\n",
            (0..90)
                .map(|i| format!("        let value_{i} = {i};"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
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

    assert_eq!(
        prepared.url,
        "https://github.com/owner/repo/blob/main/src/lib.rs"
    );
    assert_eq!(prepared.chunks.len(), prepared.chunk_extra.len());
    let doc_extra = prepared.extra.as_ref().expect("doc extra");
    assert_eq!(doc_extra["git_owner"], "owner");
    assert_eq!(doc_extra["code_language"], "rust");
    let chunk_extra = prepared
        .chunk_extra
        .iter()
        .find(|extra| extra.get("symbol_name").and_then(|v| v.as_str()) == Some("Response::parse"))
        .expect("missing chunk metadata for symbol 'Response::parse'");
    assert_eq!(chunk_extra["chunk_content_kind"], "code");
    assert!(
        chunk_extra["chunk_locator"]
            .as_str()
            .unwrap()
            .contains("src/lib.rs#L")
    );
    assert!(chunk_extra["source_range"]["line_start"].as_u64().is_some());
    assert!(chunk_extra["code_line_start"].as_u64().is_some());
    assert!(chunk_extra["code_line_end"].as_u64().is_some());
    assert!(chunk_extra["code_chunking_method"].as_str().is_some());
}

#[tokio::test]
async fn markdown_file_source_marks_chunks_as_markdown_not_code() {
    let source = SourceDocument::try_new_file(
        SourceOrigin::GitFile,
        "https://gitlab.com/group/repo/-/blob/main/README.md".to_string(),
        "README.md".to_string(),
        "md".to_string(),
        "# Readme\n\nprose content".to_string(),
        "gitlab",
        Some("README.md".to_string()),
        Some(serde_json::json!({
            "provider": "gitlab",
            "git_owner": "group",
            "git_repo": "repo",
            "git_content_kind": "file"
        })),
    )
    .expect("source doc");

    let prepared = prepare_source_document(source).await.expect("prepared doc");

    assert_eq!(prepared.content_type, "text");
    assert_eq!(
        prepared.extra.as_ref().unwrap()["code_file_path"],
        "README.md"
    );
    assert_eq!(prepared.chunk_extra[0]["chunk_content_kind"], "markdown");
    assert_eq!(prepared.chunk_extra[0]["code_chunk_source"], "markdown");
}

#[tokio::test]
async fn text_file_source_marks_chunks_as_plain_text_not_code() {
    let source = SourceDocument::try_new_file(
        SourceOrigin::GitFile,
        "https://example.com/repo#main:notes.txt".to_string(),
        "notes.txt".to_string(),
        "txt".to_string(),
        "plain notes only".to_string(),
        "git",
        Some("notes.txt".to_string()),
        Some(serde_json::json!({
            "provider": "git",
            "git_repo": "repo",
            "git_content_kind": "file"
        })),
    )
    .expect("source doc");

    let prepared = prepare_source_document(source).await.expect("prepared doc");

    assert_eq!(prepared.chunk_extra[0]["chunk_content_kind"], "plain_text");
    assert_eq!(prepared.chunk_extra[0]["code_chunk_source"], "prose");
}

#[tokio::test]
async fn memory_source_is_atomic_and_preserves_point_id() {
    let point_id = uuid::Uuid::new_v4();
    let url = format!("memory://{point_id}");
    let source = SourceDocument::new_memory(
        url.clone(),
        "Important memory text".to_string(),
        Some("Important memory".to_string()),
        Some(serde_json::json!({
            "memory": true,
            "type": "fact",
            "body": "Important memory text"
        })),
        point_id,
    );

    let prepared = prepare_source_document(source).await.expect("prepared doc");

    assert_eq!(prepared.url, url);
    assert_eq!(prepared.domain, "memory");
    assert_eq!(prepared.source_type, "memory");
    assert_eq!(prepared.content_type, "text");
    assert_eq!(prepared.chunks, vec!["Important memory text".to_string()]);
    assert_eq!(prepared.chunk_point_ids, vec![point_id]);
    assert_eq!(prepared.chunk_extra[0]["chunk_content_kind"], "plain_text");
    assert!(
        prepared.chunk_extra[0]["prepared_chunk_id"]
            .as_str()
            .is_some_and(|value| value.starts_with("chunk_"))
    );
    assert!(
        prepared.chunk_extra[0]["prepared_chunk_key"]
            .as_str()
            .is_some_and(|value| value.contains("atomic_metadata"))
    );
    assert!(
        prepared.chunk_extra[0]["prepared_content_hash"]
            .as_str()
            .is_some_and(|value| value.starts_with("fnv1a64:"))
    );
    assert_eq!(
        prepared.chunk_extra[0]["chunk_locator"],
        format!("{url}#chunk-0")
    );
    assert_eq!(prepared.extra.as_ref().unwrap()["memory"], true);
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

#[tokio::test]
async fn planner_owned_fields_are_removed_from_doc_extra() {
    let source = SourceDocument::new_plain_text(
        "reddit://post/1".to_string(),
        "reddit.com".to_string(),
        "hello".to_string(),
        "reddit",
        None,
        Some(serde_json::json!({
            "reddit_subreddit": "rust",
            "content_kind": "legacy-evil",
            "chunk_content_kind": "evil",
            "chunk_locator": "evil"
        })),
    );

    let prepared = prepare_source_document(source).await.expect("prepared doc");
    let extra = prepared.extra.as_ref().expect("extra");
    assert_eq!(extra["reddit_subreddit"], "rust");
    assert!(extra.get("content_kind").is_none());
    assert!(extra.get("chunk_content_kind").is_none());
    assert_eq!(prepared.chunk_extra[0]["chunk_content_kind"], "plain_text");
}

#[test]
fn vertical_summary_helper_caps_payload() {
    let value = serde_json::json!({"name": "ruff"});
    assert!(structured_payload_from_vertical_summary("pypi", value, 1024).is_some());
    let large = serde_json::json!({"blob": "x".repeat(2048)});
    assert!(structured_payload_from_vertical_summary("pypi", large, 8).is_none());
}
