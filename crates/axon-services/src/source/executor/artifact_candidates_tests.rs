use super::*;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone, Copy)]
enum SinkMode {
    Accepted,
    Disabled,
    Partial,
    Rejected,
    Failed,
}

#[derive(Clone)]
struct RecordingSink {
    max_batch_size: u32,
    versions: Vec<String>,
    mode: SinkMode,
    supports_idempotency: bool,
    invalid_receipt: bool,
    failure_retryable: bool,
    batches: Arc<Mutex<Vec<ArtifactCandidateBatch>>>,
    capability_calls: Arc<Mutex<u64>>,
}

impl RecordingSink {
    fn accepted(max_batch_size: u32) -> Self {
        Self {
            max_batch_size,
            versions: vec![ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION.to_string()],
            mode: SinkMode::Accepted,
            supports_idempotency: true,
            invalid_receipt: false,
            failure_retryable: false,
            batches: Arc::new(Mutex::new(Vec::new())),
            capability_calls: Arc::new(Mutex::new(0)),
        }
    }

    fn failed() -> Self {
        Self {
            mode: SinkMode::Failed,
            ..Self::accepted(64)
        }
    }

    fn retryable_failed() -> Self {
        Self {
            mode: SinkMode::Failed,
            failure_retryable: true,
            ..Self::accepted(64)
        }
    }

    fn with_mode(mode: SinkMode) -> Self {
        Self {
            mode,
            ..Self::accepted(64)
        }
    }

    fn unsupported() -> Self {
        Self {
            versions: vec!["99".to_string()],
            ..Self::accepted(64)
        }
    }

    fn without_idempotency() -> Self {
        Self {
            supports_idempotency: false,
            ..Self::accepted(64)
        }
    }

    fn with_invalid_receipt() -> Self {
        Self {
            invalid_receipt: true,
            ..Self::accepted(64)
        }
    }

    fn batches(&self) -> Vec<ArtifactCandidateBatch> {
        self.batches.lock().expect("batch mutex poisoned").clone()
    }

    fn capability_calls(&self) -> u64 {
        *self
            .capability_calls
            .lock()
            .expect("capability mutex poisoned")
    }
}

#[async_trait]
impl ArtifactCandidateSink for RecordingSink {
    async fn submit(
        &self,
        batch: ArtifactCandidateBatch,
    ) -> Result<ArtifactCandidateSinkResult, ApiError> {
        if matches!(self.mode, SinkMode::Failed) {
            self.batches
                .lock()
                .expect("batch mutex poisoned")
                .push(batch);
            let error = ApiError::new(
                "test.artifact_candidate.sink_failed",
                ErrorStage::Publishing,
                "synthetic sink failure",
            );
            return Err(if self.failure_retryable {
                error.with_retry_policy(axon_error::RetryPolicy::retryable(
                    axon_error::RetryScope::Provider,
                ))
            } else {
                error
            });
        }
        let attempted = batch.candidates.len() as u64;
        let reported_attempted = if self.invalid_receipt {
            attempted.saturating_add(1)
        } else {
            attempted
        };
        self.batches
            .lock()
            .expect("batch mutex poisoned")
            .push(batch);
        let (status, accepted, rejected) = match self.mode {
            SinkMode::Accepted => (ArtifactCandidateSinkStatus::Accepted, attempted, 0),
            SinkMode::Disabled => (ArtifactCandidateSinkStatus::Disabled, 0, 0),
            SinkMode::Partial => (ArtifactCandidateSinkStatus::Partial, attempted - 1, 1),
            SinkMode::Rejected => (ArtifactCandidateSinkStatus::Rejected, 0, attempted),
            SinkMode::Failed => unreachable!("failed mode returns before receipt construction"),
        };
        Ok(ArtifactCandidateSinkResult {
            status,
            attempted: reported_attempted,
            accepted,
            rejected,
            warnings: Vec::new(),
        })
    }

    async fn capabilities(&self) -> Result<ArtifactCandidateSinkCapability, ApiError> {
        *self
            .capability_calls
            .lock()
            .expect("capability mutex poisoned") += 1;
        Ok(ArtifactCandidateSinkCapability {
            name: "recording".to_string(),
            version: "1".to_string(),
            contract_versions: self.versions.clone(),
            max_batch_size: self.max_batch_size,
            supports_idempotency: self.supports_idempotency,
        })
    }
}

fn job_id() -> JobId {
    JobId::from(Uuid::nil())
}

fn generation() -> SourceGenerationId {
    SourceGenerationId::from("7")
}

fn source_id() -> SourceId {
    SourceId::from("src_artifacts")
}

fn source_item_key(index: usize) -> SourceItemKey {
    SourceItemKey::from(format!("skills/demo-{index}"))
}

fn document(index: usize) -> SourceDocument {
    SourceDocument {
        document_id: DocumentId::from(format!("doc-{index}")),
        source_id: source_id(),
        source_item_key: source_item_key(index),
        canonical_uri: format!("https://github.com/acme/repo/tree/main/skills/demo-{index}"),
        content_kind: ContentKind::Markdown,
        content: ContentRef::InlineText {
            text: format!("# demo {index}"),
        },
        metadata: MetadataMap::new(),
        title: Some(format!("demo-{index}")),
        language: None,
        path: Some(format!("skills/demo-{index}/SKILL.md")),
        mime_type: Some("text/markdown".to_string()),
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    }
}

fn candidate(index: usize) -> ArtifactCandidate {
    let digest = format!("{index:064x}");
    let mut manifest_metadata = MetadataMap::new();
    manifest_metadata.insert(
        "axonSourceItemKey".to_string(),
        serde_json::json!(source_item_key(index).0),
    );
    ArtifactCandidate {
        schema_version: ARTIFACT_CANDIDATE_SCHEMA_VERSION.to_string(),
        id: ArtifactCandidateId::from(format!("cand_{digest}")),
        canonical_source_uri: format!("https://github.com/acme/repo/tree/main/skills/demo-{index}"),
        source_provider: "axon".to_string(),
        observed_at: Timestamp("2026-08-19T14:00:00Z".to_string()),
        repository: Some("https://github.com/acme/repo".to_string()),
        source_ref: Some("main".to_string()),
        source_path: Some(format!("skills/demo-{index}")),
        kind_hints: vec!["skill".to_string()],
        observed_files: Vec::new(),
        manifest_metadata,
        content_digests: vec![format!("sha256:{digest}")],
        discovery_evidence: MetadataMap::new(),
        popularity_signals: MetadataMap::new(),
        license_evidence: MetadataMap::new(),
        crawl_generation_id: Some(generation().0),
        crawl_job_id: Some(job_id().0.to_string()),
        warnings: Vec::new(),
    }
}

#[test]
fn candidate_validation_rejects_wrong_correlation_and_suppresses_duplicates() {
    let documents = vec![document(1)];
    let mut wrong_job = candidate(1);
    wrong_job.crawl_job_id = Some(Uuid::new_v4().to_string());
    let mut wrong_generation = candidate(1);
    wrong_generation.id = ArtifactCandidateId::from(
        "cand_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    wrong_generation.crawl_generation_id = Some("8".to_string());
    let mut wrong_item = candidate(1);
    wrong_item.id = ArtifactCandidateId::from(
        "cand_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );
    wrong_item.manifest_metadata.insert(
        "axonSourceItemKey".to_string(),
        serde_json::json!("missing/item"),
    );
    let valid = candidate(1);
    let duplicate = valid.clone();

    let result = validate_produced_candidates(
        job_id(),
        &generation(),
        &documents,
        vec![
            wrong_job,
            wrong_generation,
            wrong_item,
            valid.clone(),
            duplicate,
        ],
    );

    assert_eq!(result.candidates, vec![valid]);
    assert_eq!(
        result
            .warnings
            .iter()
            .filter(|warning| warning.code == "source.artifact_candidate.correlation_rejected")
            .count(),
        3
    );
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.code == "source.artifact_candidate.duplicate_suppressed")
    );
}

#[test]
fn token_shaped_evidence_value_is_redacted_before_delivery() {
    let documents = vec![document(1)];
    let mut value = candidate(1);
    value.discovery_evidence.insert(
        "note".to_string(),
        serde_json::json!("request failed Authorization: Bearer abc123; retrying safely"),
    );

    let result = validate_produced_candidates(job_id(), &generation(), &documents, vec![value]);

    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(
        result.candidates[0]
            .discovery_evidence
            .get("note")
            .and_then(serde_json::Value::as_str),
        Some("request failed Authorization: Bearer [REDACTED]; retrying safely")
    );
}

#[tokio::test]
async fn committed_delivery_honors_hard_batch_ceiling_and_is_idempotent() {
    let sink = RecordingSink::accepted(1_000);
    let candidates = (0..130).map(candidate).collect::<Vec<_>>();

    let warnings = submit_committed_candidates(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        candidates.clone(),
    )
    .await;
    assert_eq!(
        warnings
            .iter()
            .filter(|warning| warning.code == "source.artifact_candidate.sink_accepted")
            .count(),
        3
    );
    let first = sink.batches();
    assert_eq!(first.len(), 3);
    assert_eq!(first[0].candidates.len(), 64);
    assert_eq!(first[1].candidates.len(), 64);
    assert_eq!(first[2].candidates.len(), 2);
    assert!(first.iter().all(|batch| {
        batch.contract_version == ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION
            && batch.job_id == job_id()
            && batch.source_id == source_id()
            && batch.generation == generation()
            && batch
                .candidates
                .iter()
                .all(|candidate| candidate.schema_version == ARTIFACT_CANDIDATE_SCHEMA_VERSION)
    }));

    let first_keys = first
        .iter()
        .map(|batch| batch.idempotency_key.clone())
        .collect::<Vec<_>>();
    let mut replay_candidates = candidates;
    replay_candidates.reverse();
    let warnings = submit_committed_candidates(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        replay_candidates,
    )
    .await;
    assert_eq!(
        warnings
            .iter()
            .filter(|warning| warning.code == "source.artifact_candidate.sink_accepted")
            .count(),
        3
    );
    let all = sink.batches();
    let replay_keys = all[3..]
        .iter()
        .map(|batch| batch.idempotency_key.clone())
        .collect::<Vec<_>>();
    assert_eq!(first_keys, replay_keys);
}

#[tokio::test]
async fn sink_smaller_batch_limit_is_respected() {
    let sink = RecordingSink::accepted(2);
    let warnings = submit_committed_candidates(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        (0..5).map(candidate).collect(),
    )
    .await;
    assert_eq!(
        warnings
            .iter()
            .filter(|warning| warning.code == "source.artifact_candidate.sink_accepted")
            .count(),
        3
    );
    assert_eq!(
        sink.batches()
            .iter()
            .map(|batch| batch.candidates.len())
            .collect::<Vec<_>>(),
        vec![2, 2, 1]
    );
}

#[tokio::test]
async fn unsupported_wrapper_contract_never_submits() {
    let sink = RecordingSink::unsupported();
    let warnings = submit_committed_candidates(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        vec![candidate(1)],
    )
    .await;
    assert!(sink.batches().is_empty());
    assert_eq!(sink.capability_calls(), 1);
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].code,
        "source.artifact_candidate.sink_contract_unsupported"
    );
}

#[tokio::test]
async fn sink_without_idempotency_support_never_submits() {
    let sink = RecordingSink::without_idempotency();
    let warnings = submit_committed_candidates(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        vec![candidate(1)],
    )
    .await;
    assert!(sink.batches().is_empty());
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].code,
        "source.artifact_candidate.sink_idempotency_unsupported"
    );
}

#[tokio::test]
async fn impossible_sink_receipt_does_not_skip_later_candidates() {
    let mut sink = RecordingSink::with_invalid_receipt();
    sink.max_batch_size = 1;
    let warnings = submit_committed_candidates(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        vec![candidate(1), candidate(2)],
    )
    .await;
    assert_eq!(sink.batches().len(), 2);
    assert!(
        warnings
            .iter()
            .any(|warning| { warning.code == "source.artifact_candidate.sink_receipt_invalid" })
    );
    assert!(
        !warnings
            .iter()
            .any(|warning| warning.code == "source.artifact_candidate.sink_delivery_skipped")
    );
}

#[tokio::test]
async fn valid_disabled_receipt_is_operator_visible() {
    let mut sink = RecordingSink::with_mode(SinkMode::Disabled);
    sink.max_batch_size = 1;
    let warnings = submit_committed_candidates(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        vec![candidate(1), candidate(2)],
    )
    .await;
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].code, "source.artifact_candidate.sink_disabled");
    assert_eq!(sink.batches().len(), 1);

    let outcome = submit_committed_candidates_with_outcome(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        vec![candidate(1), candidate(2)],
    )
    .await;
    assert_eq!(outcome.disposition, CandidateDeliveryDisposition::Disabled);
}

#[tokio::test]
async fn valid_partial_receipt_is_non_retryable_degraded_evidence_without_an_outbox() {
    let sink = RecordingSink::with_mode(SinkMode::Partial);
    let warnings = submit_committed_candidates(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        vec![candidate(1), candidate(2)],
    )
    .await;
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].code, "source.artifact_candidate.sink_partial");
    assert!(!warnings[0].retryable);
}

#[tokio::test]
async fn valid_rejected_receipt_continues_with_later_candidates() {
    let mut sink = RecordingSink::with_mode(SinkMode::Rejected);
    sink.max_batch_size = 1;
    let warnings = submit_committed_candidates(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        vec![candidate(1), candidate(2)],
    )
    .await;
    assert_eq!(sink.batches().len(), 2);
    assert!(warnings.iter().any(|warning| {
        warning.code == "source.artifact_candidate.sink_rejected" && !warning.retryable
    }));
    assert!(
        !warnings
            .iter()
            .any(|warning| warning.code == "source.artifact_candidate.sink_delivery_skipped")
    );
}

#[tokio::test]
async fn generation_wide_duplicates_are_suppressed_and_id_collisions_are_rejected() {
    let sink = RecordingSink::accepted(64);
    let duplicate = candidate(1);
    let mut collision = duplicate.clone();
    collision.canonical_source_uri = "https://github.com/acme/other".to_string();

    let warnings = submit_committed_candidates(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        vec![duplicate.clone(), duplicate, collision, candidate(2)],
    )
    .await;

    assert_eq!(sink.batches().len(), 1);
    assert_eq!(sink.batches()[0].candidates, vec![candidate(2)]);
    assert!(
        warnings
            .iter()
            .any(|warning| { warning.code == "source.artifact_candidate.duplicate_suppressed" })
    );
    assert!(warnings.iter().any(|warning| {
        warning.code == "source.artifact_candidate.identity_collision_rejected"
    }));
}

#[tokio::test]
async fn sink_failure_degrades_without_panicking_or_reclassifying_candidates() {
    let sink = RecordingSink::failed();
    let warnings = submit_committed_candidates(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        vec![candidate(1)],
    )
    .await;
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].code, "source.artifact_candidate.sink_failed");
    assert!(!warnings[0].retryable);
}

#[tokio::test]
async fn terminal_sink_failure_continues_with_later_candidates() {
    let sink = RecordingSink {
        max_batch_size: 1,
        ..RecordingSink::failed()
    };
    let warnings = submit_committed_candidates(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        vec![candidate(1), candidate(2), candidate(3)],
    )
    .await;

    assert_eq!(warnings.len(), 3);
    assert!(warnings.iter().all(|warning| !warning.retryable));
    assert_eq!(sink.batches().len(), 3);
}

#[tokio::test]
async fn retryable_sink_failure_stops_and_reports_remaining_batch_delivery() {
    let sink = RecordingSink {
        max_batch_size: 1,
        ..RecordingSink::retryable_failed()
    };
    let warnings = submit_committed_candidates(
        &sink,
        job_id(),
        source_id(),
        &generation(),
        vec![candidate(1), candidate(2), candidate(3)],
    )
    .await;

    assert_eq!(sink.batches().len(), 1);
    assert!(warnings[0].retryable);
    assert_eq!(
        warnings[1].code,
        "source.artifact_candidate.sink_delivery_skipped"
    );
    assert!(warnings[1].message.contains("2 candidates"));
}

#[tokio::test]
async fn empty_candidate_set_does_not_probe_or_submit_sink() {
    let sink = RecordingSink::accepted(64);
    let warnings =
        submit_committed_candidates(&sink, job_id(), source_id(), &generation(), Vec::new()).await;
    assert!(warnings.is_empty());
    assert_eq!(sink.capability_calls(), 0);
    assert!(sink.batches().is_empty());
}

#[tokio::test]
async fn rejected_receipt_is_a_typed_terminal_delivery() {
    let outcome = submit_committed_candidates_with_outcome(
        &RecordingSink::with_mode(SinkMode::Rejected),
        job_id(),
        source_id(),
        &generation(),
        vec![candidate(0)],
    )
    .await;

    assert_eq!(outcome.disposition, CandidateDeliveryDisposition::Terminal);
}

#[tokio::test]
async fn retry_policy_controls_delivery_disposition() {
    let outcome = submit_committed_candidates_with_outcome(
        &RecordingSink::retryable_failed(),
        job_id(),
        source_id(),
        &generation(),
        vec![candidate(0)],
    )
    .await;

    assert_eq!(outcome.disposition, CandidateDeliveryDisposition::Retryable);
    assert!(outcome.warnings.iter().any(|warning| {
        warning.code == "source.artifact_candidate.sink_failed" && warning.retryable
    }));
}
