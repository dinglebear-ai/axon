use super::*;

#[tokio::test]
async fn shutdown_aborts_and_joins_registered_drain() {
    let root = tempfile::tempdir().expect("tempdir");
    let outbox = ArtifactCandidateOutbox::new(root.path());
    assert!(outbox.begin_drain());
    let task = tokio::spawn(std::future::pending::<()>());
    outbox.register_drain_task(task);

    tokio::time::timeout(
        std::time::Duration::from_millis(100),
        outbox.shutdown_drain(),
    )
    .await
    .expect("supervised shutdown must be bounded");

    assert!(!outbox.draining.load(Ordering::Acquire));
    assert!(outbox.drain_cancelled());
}
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
async fn legacy_corrupt_entry_is_quarantined_without_blocking_valid_delivery() {
    let directory = tempdir().expect("tempdir");
    let outbox = ArtifactCandidateOutbox::new(directory.path());
    // Legacy files are reconciled on first access; current writers reserve every name.
    tokio::fs::write(
        directory.path().join(format!("{}.json", "a".repeat(64))),
        b"not-json",
    )
    .await
    .expect("write corrupt entry");
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

#[test]
fn retry_exhaustion_releases_runner_without_consuming_queued_drain() {
    let outbox = ArtifactCandidateOutbox::new("unused");
    assert!(outbox.begin_drain());
    outbox.start_drain_pass();

    // A producer queues another supervised pass while the current pass is
    // consuming its final retry attempt.
    assert!(!outbox.begin_drain());
    assert!(
        outbox.finish_exhausted_drain(),
        "the active runner must own the queued follow-up pass"
    );

    outbox.start_drain_pass();
    assert!(!outbox.finish_exhausted_drain());
    assert!(
        outbox.begin_drain(),
        "runner exit after the follow-up pass must release drain ownership"
    );
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

#[test]
fn outbox_encoder_rejects_before_allocating_the_complete_json() {
    assert!(bounded_encode(&"x".repeat(4096), 1024).is_err());
    assert_eq!(bounded_encode(&vec![1, 2, 3], 7).unwrap(), b"[1,2,3]");
}

#[tokio::test]
async fn aggregate_scan_budget_defers_pages_without_starving_any_delivery() {
    let directory = tempdir().unwrap();
    let outbox = ArtifactCandidateOutbox::new(directory.path());
    let mut expected = std::collections::BTreeSet::new();
    let mut largest = 0;
    for index in 0..12 {
        let mut value = candidate();
        value.id = ArtifactCandidateId::new(format!("candidate-{index}"));
        value.warnings.push("x".repeat(1024));
        let record = outbox
            .stage(
                JobId::from(uuid::Uuid::from_u128(1)),
                SourceId::new("source"),
                SourceGenerationId::new("generation"),
                vec![value],
            )
            .await
            .unwrap()
            .unwrap();
        let size = tokio::fs::metadata(outbox.path(&record.delivery_key))
            .await
            .unwrap()
            .len();
        largest = largest.max(size);
        expected.insert(record.delivery_key);
    }
    let budget = largest * 3;
    let mut observed = std::collections::BTreeSet::new();
    let mut passes = 0;
    loop {
        let scan = outbox.scan_with_budget(budget).await.unwrap();
        let mut bytes = 0;
        for delivery in scan.deliveries {
            bytes += tokio::fs::metadata(outbox.path(&delivery.delivery_key))
                .await
                .unwrap()
                .len();
            assert!(
                observed.insert(delivery.delivery_key),
                "one cycle must not repeat a page"
            );
        }
        assert!(bytes <= budget);
        passes += 1;
        if !scan.has_more {
            break;
        }
        assert!(passes < 20);
    }
    assert!(passes >= 4);
    assert_eq!(observed, expected);
    assert!(
        !outbox
            .scan_with_budget(budget)
            .await
            .unwrap()
            .deliveries
            .is_empty(),
        "next cycle restarts at the beginning"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn scan_never_follows_a_payload_symlink() {
    let directory = tempdir().unwrap();
    let external = tempdir().unwrap();
    let target = external.path().join("payload");
    tokio::fs::write(&target, b"private external data")
        .await
        .unwrap();
    let link = directory.path().join(format!("{}.json", "a".repeat(64)));
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let outbox = ArtifactCandidateOutbox::new(directory.path());
    let scan = outbox.scan().await.unwrap();
    assert!(scan.deliveries.is_empty());
    assert_eq!(
        scan.findings[0].code,
        "artifact_candidate.outbox.invalid_file"
    );
    assert_eq!(
        tokio::fs::read(&target).await.unwrap(),
        b"private external data"
    );
}

#[tokio::test]
async fn disk_admission_counts_quarantined_and_incomplete_files_without_deleting_them() {
    let directory = tempdir().unwrap();
    tokio::fs::write(directory.path().join("orphan.tmp"), vec![0; 500])
        .await
        .unwrap();
    tokio::fs::write(directory.path().join("corrupt.invalid"), vec![0; 500])
        .await
        .unwrap();
    let mut quota = quota::open(directory.path(), true).unwrap();
    let next = directory.path().join("next.json");
    assert!(quota::reserve(&mut quota, &next, 100, 1099).is_err());
    quota::reserve(&mut quota, &next, 100, 1100).unwrap();
    assert_eq!(
        std::fs::metadata(directory.path().join("orphan.tmp"))
            .unwrap()
            .len(),
        500
    );
    assert_eq!(
        std::fs::metadata(directory.path().join("corrupt.invalid"))
            .unwrap()
            .len(),
        500
    );
    assert!(!next.exists());
    let owner = lock_disk_admission(directory.path()).unwrap();
    assert!(lock_disk_admission(directory.path()).is_err());
    drop(owner);
    assert!(lock_disk_admission(directory.path()).is_ok());
}

#[tokio::test]
async fn aggregate_decoded_cost_pages_even_when_wire_bytes_fit() {
    let root = tempdir().unwrap();
    let outbox = ArtifactCandidateOutbox::new(root.path());
    let mut expected = std::collections::BTreeSet::new();
    let mut largest_charge = 0;
    for index in 0..9 {
        let mut value = candidate();
        value.id = ArtifactCandidateId::new(format!("candidate-{index}"));
        let delivery = outbox
            .stage(
                JobId::new(uuid::Uuid::from_u128(1)),
                SourceId::new("s"),
                SourceGenerationId::new("g"),
                vec![value],
            )
            .await
            .unwrap()
            .unwrap();
        let bytes = tokio::fs::read(outbox.path(&delivery.delivery_key))
            .await
            .unwrap();
        largest_charge = largest_charge.max(structure::charge(&bytes).unwrap());
        expected.insert(delivery.delivery_key);
    }
    let limit = largest_charge * 2;
    let mut observed = std::collections::BTreeSet::new();
    let mut pages = 0;
    loop {
        let scan = outbox
            .scan_with_limits(MAX_OUTBOX_FILE_BYTES, limit)
            .await
            .unwrap();
        let mut cost = 0;
        for delivery in scan.deliveries {
            let bytes = tokio::fs::read(outbox.path(&delivery.delivery_key))
                .await
                .unwrap();
            cost += structure::charge(&bytes).unwrap();
            assert!(observed.insert(delivery.delivery_key));
        }
        assert!(cost <= limit);
        pages += 1;
        if !scan.has_more {
            break;
        }
        assert!(pages < 20);
    }
    assert!(pages >= 5);
    assert_eq!(observed, expected);
}

#[tokio::test]
async fn complex_legacy_delivery_is_preserved_with_an_explicit_finding() {
    let root = tempdir().unwrap();
    let outbox = ArtifactCandidateOutbox::new(root.path());
    let mut value = candidate();
    value.observed_files = vec![serde_json::json!({}); 70_000];
    let job = JobId::new(uuid::Uuid::from_u128(1));
    let source = SourceId::new("s");
    let generation = SourceGenerationId::new("g");
    let key = artifact_candidate_batch_idempotency_key(
        &job,
        &source,
        &generation,
        std::slice::from_ref(&value),
    );
    let delivery = PendingArtifactCandidateDelivery {
        delivery_key: key.clone(),
        job_id: job,
        source_id: source,
        generation,
        candidates: vec![value],
        staged_at_unix_ms: 0,
    };
    let path = outbox.path(&key);
    tokio::fs::write(&path, serde_json::to_vec(&delivery).unwrap())
        .await
        .unwrap();
    let scan = outbox.scan().await.unwrap();
    assert!(scan.deliveries.is_empty());
    assert_eq!(
        scan.findings[0].code,
        "artifact_candidate.outbox.structure_limit"
    );
    assert!(path.exists());
}

#[tokio::test]
async fn indexed_payload_corruption_is_detected_without_reopening() {
    let directory = tempdir().unwrap();
    let outbox = ArtifactCandidateOutbox::new(directory.path());
    let first = outbox
        .stage(
            JobId::new(uuid::Uuid::from_u128(901)),
            SourceId::new("source-a"),
            SourceGenerationId::new("1"),
            vec![candidate()],
        )
        .await
        .unwrap()
        .unwrap();
    let second = outbox
        .stage(
            JobId::new(uuid::Uuid::from_u128(902)),
            SourceId::new("source-a"),
            SourceGenerationId::new("1"),
            vec![candidate()],
        )
        .await
        .unwrap()
        .unwrap();
    tokio::fs::write(outbox.path(&first.delivery_key), b"not-json")
        .await
        .unwrap();
    let scan = outbox.scan().await.unwrap();
    assert_eq!(scan.deliveries.len(), 1);
    assert_eq!(scan.deliveries[0].delivery_key, second.delivery_key);
    assert_eq!(scan.findings[0].code, "artifact_candidate.outbox.corrupt");
}
