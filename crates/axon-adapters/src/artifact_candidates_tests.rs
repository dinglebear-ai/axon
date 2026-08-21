use super::*;
use axon_api::source::{
    ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION, ArtifactCandidateBatch, ArtifactCandidateSinkStatus,
    JobId, SourceGenerationId, SourceId, Timestamp,
};
use uuid::Uuid;

#[test]
fn dedupe_keys_are_stable_and_content_sensitive() {
    let first = artifact_candidate_dedupe(
        "https://github.com/acme/repo",
        Some("abc123"),
        Some("skills/demo"),
        Some("sha256:one"),
    );
    let same = artifact_candidate_dedupe(
        "https://github.com/acme/repo",
        Some("abc123"),
        Some("skills/demo"),
        Some("sha256:one"),
    );
    let changed = artifact_candidate_dedupe(
        "https://github.com/acme/repo",
        Some("abc123"),
        Some("skills/demo"),
        Some("sha256:two"),
    );

    assert_eq!(first, same);
    assert_eq!(first.identity_key, changed.identity_key);
    assert_ne!(first.content_key, changed.content_key);
    let content_key = first.content_key.as_deref().expect("content key");
    assert_eq!(
        artifact_candidate_id(&first).0,
        format!("cand_{}", content_key.trim_start_matches("sha256:"))
    );
}

#[test]
fn duplicate_evidence_keeps_exact_and_semantic_signals_separate() {
    let dedupe = artifact_candidate_dedupe(
        "https://github.com/acme/repo",
        Some("main"),
        Some("skills/demo"),
        Some("sha256:one"),
    );
    let mut neighbors = (0..40)
        .rev()
        .map(|index| ArtifactCandidateId::from(format!("cand_neighbor_{index:02}")))
        .collect::<Vec<_>>();
    neighbors.push(ArtifactCandidateId::from("cand_neighbor_00"));
    let evidence =
        artifact_candidate_duplicate_evidence(&dedupe, Some(("skills.sh", true)), &neighbors);

    assert_eq!(evidence["exact"]["identityKey"], dedupe.identity_key);
    assert_eq!(
        evidence["exact"]["contentKey"].as_str(),
        dedupe.content_key.as_deref()
    );
    let near = evidence["nearDuplicateCandidateIds"]
        .as_array()
        .expect("near duplicate ids");
    assert_eq!(near.len(), ARTIFACT_CANDIDATE_MAX_NEAR_DUPLICATES);
    assert_eq!(
        near.first().and_then(serde_json::Value::as_str),
        Some("cand_neighbor_00")
    );
    assert_eq!(
        near.last().and_then(serde_json::Value::as_str),
        Some("cand_neighbor_31")
    );
    assert_eq!(evidence["nearDuplicateTruncated"], true);
    assert_eq!(evidence["providerSignals"][0]["value"], true);
    assert_eq!(evidence["authorityScope"], "evidence-only");
    assert_eq!(
        artifact_candidate_id(&dedupe).0,
        format!(
            "cand_{}",
            dedupe
                .content_key
                .as_deref()
                .expect("content key")
                .trim_start_matches("sha256:")
        )
    );
}

#[test]
fn length_prefixing_prevents_delimiter_style_identity_aliases() {
    let left = artifact_candidate_dedupe("https://example.test/a", Some("b/c"), None, None);
    let right = artifact_candidate_dedupe("https://example.test/a/b", Some("c"), None, None);
    assert_ne!(left.identity_key, right.identity_key);
}

#[test]
fn candidate_id_falls_back_to_source_identity_without_content_hash() {
    let dedupe = artifact_candidate_dedupe(
        "https://github.com/acme/repo",
        Some("main"),
        Some("skills/demo"),
        None,
    );
    assert!(dedupe.content_key.is_none());
    assert_eq!(
        artifact_candidate_id(&dedupe).0,
        format!("cand_{}", dedupe.identity_key.trim_start_matches("sha256:"))
    );
}

#[tokio::test]
async fn noop_sink_is_explicitly_disabled_without_rejecting_pipeline_work() {
    let sink = NoopArtifactCandidateSink;
    let result = sink
        .submit(ArtifactCandidateBatch {
            contract_version: ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION.to_string(),
            delivery_id: "delivery-1".to_string(),
            idempotency_key: "idem-1".to_string(),
            job_id: JobId::from(Uuid::nil()),
            source_id: SourceId::from("src"),
            generation: SourceGenerationId::from("1"),
            produced_at: Timestamp("2026-08-19T13:45:00Z".to_string()),
            candidates: Vec::new(),
        })
        .await
        .expect("noop sink succeeds");

    assert_eq!(result.status, ArtifactCandidateSinkStatus::Disabled);
    assert_eq!(result.accepted, 0);
    assert_eq!(result.rejected, 0);

    let capability = sink.capabilities().await.expect("capability succeeds");
    assert!(
        capability
            .contract_versions
            .contains(&ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION.to_string())
    );
    assert!(capability.supports_idempotency);
}
