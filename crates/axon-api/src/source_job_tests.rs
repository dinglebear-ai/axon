use chrono::Utc;

use super::*;

fn now() -> Timestamp {
    Timestamp::from(Utc::now())
}

fn counts() -> StageCounts {
    StageCounts {
        items_total: Some(10),
        items_done: 4,
        documents_total: Some(3),
        documents_done: 2,
        chunks_total: Some(20),
        chunks_done: 8,
        bytes_total: Some(4096),
        bytes_done: 1024,
    }
}

#[test]
fn source_progress_event_carries_full_observability_context() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let event = SourceProgressEvent {
        event_id: "evt_1".to_string(),
        sequence: 7,
        job_id,
        attempt: 2,
        stage_id: Some(StageId(uuid::Uuid::new_v4())),
        batch_id: Some(BatchId(uuid::Uuid::new_v4())),
        reservation_id: Some(ReservationId::from("res_embedding_1")),
        checkpoint_id: Some(CheckpointId::from("ckpt_embedding_1")),
        dedupe_key: Some("job:event:embedding:1".to_string()),
        phase: PipelinePhase::Embedding,
        status: LifecycleStatus::Waiting,
        severity: Severity::Info,
        visibility: Visibility::Internal,
        message: "waiting for embedding capacity".to_string(),
        timestamp: now(),
        source_id: Some(SourceId::from("src_local")),
        canonical_uri: Some("file:///workspace/axon".to_string()),
        adapter: Some(AdapterRef {
            name: "local".to_string(),
            version: "0.1.0".to_string(),
        }),
        scope: Some(SourceScope::Directory),
        generation: Some(SourceGenerationId::from("gen_0001")),
        counts: counts(),
        timing: Some(ProgressTiming {
            started_at: now(),
            updated_at: now(),
            elapsed_ms: 2500,
            eta_ms: Some(1000),
        }),
        current: Some(ProgressCurrent {
            source_item_key: Some(SourceItemKey::from("src/lib.rs")),
            document_id: Some(DocumentId::from("doc_src_lib_rs")),
            chunk_id: Some(ChunkId::from("chunk_src_lib_rs_1")),
            adapter: Some("local".to_string()),
            provider: Some(ProviderId::from("tei")),
            message: Some("embedding chunk".to_string()),
        }),
        throughput: Some(ProgressThroughput {
            items_per_second: Some(2.0),
            bytes_per_second: Some(1024.0),
            chunks_per_second: Some(8.0),
        }),
        retry: None,
        warning: None,
        error: None,
    };

    let value = serde_json::to_value(&event).expect("event json");
    assert_eq!(value["attempt"], 2);
    assert_eq!(value["stage_id"], event.stage_id.unwrap().0.to_string());
    assert_eq!(value["reservation_id"], "res_embedding_1");
    assert_eq!(value["checkpoint_id"], "ckpt_embedding_1");
    assert_eq!(value["timing"]["elapsed_ms"], 2500);
    assert_eq!(value["current"]["document_id"], "doc_src_lib_rs");
    assert_eq!(value["current"]["chunk_id"], "chunk_src_lib_rs_1");
    assert_eq!(value["current"]["provider"], "tei");

    assert_eq!(
        serde_json::from_value::<SourceProgressEvent>(value).unwrap(),
        event
    );
}

#[test]
fn target_job_management_dtos_round_trip() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let parent_job_id = JobId(uuid::Uuid::new_v4());
    let source_id = SourceId::from("src_local");
    let stage_id = StageId(uuid::Uuid::new_v4());
    let created_at = now();
    let stage = JobStageSnapshot {
        stage_id,
        phase: PipelinePhase::Embedding,
        status: LifecycleStatus::Running,
        required: true,
        provider_requirements: vec![ProviderRequirement {
            provider_kind: ProviderKind::Embedding,
            capability: "embed".to_string(),
            required: true,
            reason: "embed changed chunks".to_string(),
        }],
        counts: counts(),
        started_at: Some(created_at.clone()),
        completed_at: None,
        error: None,
    };
    let heartbeat = JobHeartbeat {
        job_id,
        attempt: 1,
        worker_id: Some("worker_1".to_string()),
        phase: PipelinePhase::Embedding,
        status: LifecycleStatus::Running,
        stage_id: Some(stage_id),
        heartbeat_at: created_at.clone(),
        last_event_sequence: Some(12),
        counts: Some(counts()),
        provider_reservations: vec![ProviderReservationSnapshot {
            reservation_id: ReservationId::from("res_embedding_1"),
            provider_kind: ProviderKind::Embedding,
            provider_id: Some(ProviderId::from("tei")),
            priority: JobPriority::Background,
            requested_units: 8,
            granted_units: 4,
            acquired_at: Some(created_at.clone()),
            expires_at: Some(created_at.clone()),
            status: ProviderReservationStatus::Active,
            queue_depth: Some(3),
            cooling: Some(ProviderCoolingSnapshot {
                reason: "provider overloaded".to_string(),
                started_at: created_at.clone(),
                retry_after: Some(created_at.clone()),
                degraded: true,
            }),
        }],
    };
    let summary = JobSummary {
        job_id,
        kind: JobKind::Source,
        intent: None,
        status: LifecycleStatus::Running,
        phase: PipelinePhase::Embedding,
        created_at: created_at.clone(),
        updated_at: created_at.clone(),
        started_at: None,
        finished_at: None,
        source_id: Some(source_id.clone()),
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 0,
        priority: JobPriority::Normal,
        counts: Some(counts()),
        current: None,
        heartbeat: None,
        last_error: None,
        warnings: Vec::new(),
    };
    let status = SourceJobStatus {
        summary: summary.clone(),
        attempts: vec![JobAttemptSnapshot {
            attempt: 1,
            status: LifecycleStatus::Running,
            worker_id: Some("worker_1".to_string()),
            started_at: created_at.clone(),
            finished_at: None,
            heartbeat_at: Some(created_at.clone()),
            error: None,
        }],
        stages: vec![stage],
        latest_event_sequence: Some(12),
        poll_after_ms: Some(1000),
        metadata: MetadataMap::default(),
    };
    let create = JobCreateRequest {
        request_id: Some("req_refresh".to_string()),
        job_kind: JobKind::Source,
        job_intent: JobIntent::Refresh,
        source_id: Some(source_id),
        watch_id: None,
        parent_job_id: Some(parent_job_id),
        root_job_id: Some(parent_job_id),
        priority: JobPriority::Background,
        idempotency_key: Some("refresh:src_local".to_string()),
        stage_plan: vec![JobStagePlan {
            phase: PipelinePhase::Embedding,
            required: true,
            provider_requirements: Vec::new(),
            estimated_items: Some(10),
        }],
        request: Some(serde_json::json!({ "source": "/workspace/axon" })),
        auth_snapshot: MetadataMap::default(),
        config_snapshot_id: Some(ConfigSnapshotId::from("cfg_test")),
        requirements: MetadataMap::default(),
        result_schema: Some("source_result".to_string()),
        metadata: MetadataMap::default(),
    };
    let event_page = JobEventPage {
        events: vec![JobEvent {
            event_id: "evt_12".to_string(),
            sequence: 12,
            job_id,
            attempt: 1,
            stage_id: Some(stage_id),
            phase: PipelinePhase::Embedding,
            status: LifecycleStatus::Running,
            severity: Severity::Info,
            visibility: Visibility::Internal,
            message: "embedding".to_string(),
            timestamp: created_at.clone(),
            details: MetadataMap::default(),
        }],
        limit: 50,
        next_cursor: None,
        last_sequence: 12,
    };
    let cancel = JobCancelRequest {
        reason: Some("user requested".to_string()),
        force_after_ms: Some(30_000),
    };
    let retry = JobRetryRequest {
        mode: JobRetryMode::SameConfig,
        from_phase: Some(PipelinePhase::Embedding),
        idempotency_key: Some("retry:job".to_string()),
        overrides: MetadataMap::default(),
    };
    let cleanup = JobCleanupResult {
        matched: 3,
        deleted: 3,
        dry_run: false,
        warnings: Vec::new(),
        jobs_pruned: 3,
        events_pruned: 0,
        heartbeats_pruned: 0,
        artifacts_pruned: 0,
    };

    for value in [
        serde_json::to_value(&create).unwrap(),
        serde_json::to_value(&status).unwrap(),
        serde_json::to_value(&heartbeat).unwrap(),
        serde_json::to_value(&event_page).unwrap(),
        serde_json::to_value(&cancel).unwrap(),
        serde_json::to_value(&retry).unwrap(),
        serde_json::to_value(&cleanup).unwrap(),
    ] {
        assert!(value.is_object());
    }

    assert_eq!(
        serde_json::from_value::<SourceJobStatus>(serde_json::to_value(&status).unwrap()).unwrap(),
        status
    );
    assert_eq!(
        serde_json::from_value::<JobHeartbeat>(serde_json::to_value(&heartbeat).unwrap()).unwrap(),
        heartbeat
    );
}

#[test]
fn job_dtos_reject_unknown_fields() {
    let bad = serde_json::json!({
        "reason": "stop",
        "force_after_ms": 1000,
        "force": true
    });

    let err = serde_json::from_value::<JobCancelRequest>(bad)
        .expect_err("target job cancel request rejects legacy force field");
    assert!(err.to_string().contains("unknown field"), "{err}");
}
