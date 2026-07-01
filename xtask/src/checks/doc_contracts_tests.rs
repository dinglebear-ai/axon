use super::*;

use std::fs;

#[test]
fn removed_tokens_are_the_request_type_names_without_quotes() {
    let tokens = removed_doc_type_tokens();
    assert!(tokens.contains(&"EmbedRequest".to_string()));
    assert!(tokens.contains(&"IngestRequest".to_string()));
    assert!(tokens.contains(&"CrawlRequest".to_string()));
    // The command/route words must NOT be in the doc-prose token set.
    assert!(!tokens.iter().any(|t| t == "embed" || t == "/v1/embed"));
}

#[test]
fn contains_word_respects_identifier_boundaries() {
    assert!(contains_word("see EmbedRequest here", "EmbedRequest"));
    assert!(contains_word("`EmbedRequest`", "EmbedRequest"));
    assert!(!contains_word("MyEmbedRequestExt", "EmbedRequest"));
    assert!(!contains_word("EmbedRequestable", "EmbedRequest"));
}

#[test]
fn check_passes_on_clean_docs() {
    let root = tempfile::tempdir().unwrap();
    let ref_dir = root.path().join("docs/reference/api");
    fs::create_dir_all(&ref_dir).unwrap();
    fs::write(
        ref_dir.join("dto.md"),
        "The SourceRequest replaces the old shapes.",
    )
    .unwrap();
    check(root.path()).expect("clean docs pass");
}

#[test]
fn check_fails_when_removed_type_leaks_into_docs() {
    let root = tempfile::tempdir().unwrap();
    let ref_dir = root.path().join("docs/reference/api");
    fs::create_dir_all(&ref_dir).unwrap();
    fs::write(ref_dir.join("dto.md"), "Use `EmbedRequest` to embed.").unwrap();
    let err = check(root.path()).expect_err("leaked removed type must fail");
    assert!(err.to_string().contains("EmbedRequest"));
}

#[test]
fn check_skips_when_reference_dir_absent() {
    let root = tempfile::tempdir().unwrap();
    check(root.path()).expect("absent docs/reference is not a failure");
}
