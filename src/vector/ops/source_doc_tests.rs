use super::{
    SourceDocument, SourceOrigin, prepare_source_document, structured_payload_from_vertical_summary,
};
use crate::vector::ops::tei::StructuredPayload;

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
        .find(|extra| extra.get("symbol_name").and_then(|v| v.as_str()) == Some("Parser::parse"))
        .unwrap_or_else(|| prepared.chunk_extra.first().expect("chunk metadata"));
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
