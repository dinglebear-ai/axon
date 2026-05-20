use super::artifacts::{ArtifactHandle, ArtifactKind};
use uuid::Uuid;

#[test]
fn artifact_handle_rejects_parent_dir_relative_path() {
    let result = ArtifactHandle::new(
        ArtifactKind::Markdown,
        "../secret.md",
        Some("https://example.com".to_string()),
        None,
        "abc123".to_string(),
        12,
        Some(1),
        None,
    );

    assert!(result.is_err());
}

#[test]
fn artifact_id_is_stable_for_kind_path_and_hash() {
    let one = ArtifactHandle::new(
        ArtifactKind::CrawlManifest,
        "domains/example.com/job-1/manifest.jsonl",
        Some("https://example.com".to_string()),
        Some(Uuid::nil()),
        "abc123".to_string(),
        128,
        Some(4),
        Some("/home/axon/.axon/output/domains/example.com/job-1/manifest.jsonl".to_string()),
    )
    .expect("artifact handle");
    let two = ArtifactHandle::new(
        ArtifactKind::CrawlManifest,
        "domains/example.com/job-1/manifest.jsonl",
        Some("https://example.com".to_string()),
        Some(Uuid::nil()),
        "abc123".to_string(),
        128,
        Some(4),
        None,
    )
    .expect("artifact handle");

    assert_eq!(one.artifact_id, two.artifact_id);
    assert_eq!(one.kind, ArtifactKind::CrawlManifest);
}
