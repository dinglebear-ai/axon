use super::*;
use axon_api::source::{
    ARTIFACT_CANDIDATE_SCHEMA_VERSION, ArtifactCandidateId, MetadataMap, Timestamp,
};
use tempfile::tempdir;

fn candidate() -> ArtifactCandidate {
    ArtifactCandidate {
        schema_version: ARTIFACT_CANDIDATE_SCHEMA_VERSION.to_string(),
        id: ArtifactCandidateId::new("candidate-a"),
        canonical_source_uri: "https://example.com/candidate-a".to_string(),
        source_provider: "axon".to_string(),
        observed_at: Timestamp("2026-08-20T00:00:00Z".to_string()),
        repository: None,
        source_ref: None,
        source_path: None,
        kind_hints: vec!["skill".to_string()],
        observed_files: Vec::new(),
        manifest_metadata: MetadataMap::new(),
        content_digests: Vec::new(),
        discovery_evidence: MetadataMap::new(),
        popularity_signals: MetadataMap::new(),
        license_evidence: MetadataMap::new(),
        crawl_job_id: Some("00000000-0000-0000-0000-000000000001".to_string()),
        crawl_generation_id: Some("gen-1".to_string()),
        warnings: Vec::new(),
    }
}

#[tokio::test]
async fn staged_delivery_survives_reopen_and_completion_is_idempotent() {
    let directory = tempdir().expect("tempdir");
    let first = ArtifactCandidateOutbox::new(directory.path());
    let pending = first
        .stage(
            JobId::from(uuid::Uuid::from_u128(1)),
            SourceId::new("source-a"),
            SourceGenerationId::new("gen-1"),
            vec![candidate()],
        )
        .await
        .expect("stage")
        .expect("pending");

    let reopened = ArtifactCandidateOutbox::new(directory.path());
    let recovered = reopened.pending().await.expect("pending list");
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].delivery_key, pending.delivery_key);

    reopened
        .complete(&pending.delivery_key)
        .await
        .expect("complete");
    reopened
        .complete(&pending.delivery_key)
        .await
        .expect("idempotent complete");
    assert!(reopened.pending().await.expect("empty list").is_empty());
}

#[tokio::test]
async fn completion_rejects_path_traversal_keys() {
    let directory = tempdir().expect("tempdir");
    let outbox = ArtifactCandidateOutbox::new(directory.path());
    let error = outbox
        .complete("../../outside")
        .await
        .expect_err("invalid key must fail");
    assert!(
        error
            .to_string()
            .contains("invalid artifact candidate delivery key")
    );
}

#[tokio::test]
async fn corrupt_entry_is_quarantined_without_blocking_valid_delivery() {
    let directory = tempdir().expect("tempdir");
    let outbox = ArtifactCandidateOutbox::new(directory.path());
    let staged = outbox
        .stage(
            JobId::from(uuid::Uuid::from_u128(1)),
            SourceId::new("source-a"),
            SourceGenerationId::new("gen-1"),
            vec![candidate()],
        )
        .await
        .expect("stage")
        .expect("pending");
    tokio::fs::write(
        directory.path().join(format!("{}.json", "a".repeat(64))),
        b"not-json",
    )
    .await
    .expect("write corrupt entry");

    let scan = outbox.scan().await.expect("scan continues");
    assert_eq!(scan.deliveries.len(), 1);
    assert_eq!(scan.deliveries[0].delivery_key, staged.delivery_key);
    assert_eq!(scan.findings.len(), 1);
    assert_eq!(scan.findings[0].code, "artifact_candidate.outbox.corrupt");
    assert_eq!(
        scan.findings[0].file_name,
        format!("{}.json", "a".repeat(64))
    );
    let names = std::fs::read_dir(directory.path())
        .expect("read directory")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    assert!(names.iter().any(|name| name.contains(".invalid.")));
}

#[tokio::test]
async fn scan_cap_rotates_so_later_entries_are_not_starved() {
    let directory = tempdir().expect("tempdir");
    for index in 0..=MAX_PENDING_DELIVERIES {
        let name = format!("{index:064x}.json");
        tokio::fs::write(directory.path().join(name), b"not-json")
            .await
            .expect("write entry");
    }
    let outbox = ArtifactCandidateOutbox::new(directory.path());
    assert!(outbox.pending().await.expect("first page").is_empty());
    assert!(outbox.pending().await.expect("second page").is_empty());

    let json_entries = std::fs::read_dir(directory.path())
        .expect("read directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .count();
    assert_eq!(json_entries, 0);
}

#[test]
fn drain_request_is_not_lost_during_an_active_pass() {
    let outbox = ArtifactCandidateOutbox::new("unused");
    assert!(outbox.begin_drain());
    outbox.start_drain_pass();
    assert!(!outbox.begin_drain());
    assert!(outbox.continue_or_finish_drain());
    outbox.start_drain_pass();
    assert!(!outbox.continue_or_finish_drain());
}

#[cfg(unix)]
#[tokio::test]
async fn staged_entries_are_private() {
    use std::os::unix::fs::PermissionsExt;

    let parent = tempdir().expect("tempdir");
    let root = parent.path().join("outbox");
    let outbox = ArtifactCandidateOutbox::new(&root);
    let staged = outbox
        .stage(
            JobId::from(uuid::Uuid::from_u128(1)),
            SourceId::new("source-a"),
            SourceGenerationId::new("gen-1"),
            vec![candidate()],
        )
        .await
        .expect("stage")
        .expect("pending");
    assert_eq!(
        std::fs::metadata(&root).expect("root").permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(root.join(format!(
            "{}.json",
            staged.delivery_key.strip_prefix("sha256:").expect("digest")
        )))
        .expect("entry")
        .permissions()
        .mode()
            & 0o777,
        0o600
    );
}
