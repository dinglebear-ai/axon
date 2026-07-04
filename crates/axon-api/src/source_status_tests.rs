use chrono::Utc;

use super::*;

#[test]
fn status_envelope_and_progress_event_round_trip() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let counts = StageCounts {
        items_total: Some(10),
        items_done: 4,
        documents_total: Some(4),
        documents_done: 1,
        chunks_total: None,
        chunks_done: 0,
        bytes_total: None,
        bytes_done: 0,
    };
    let event = SourceProgressEvent {
        event_id: "evt_1".to_string(),
        sequence: 1,
        job_id,
        attempt: 1,
        stage_id: None,
        batch_id: None,
        reservation_id: None,
        checkpoint_id: None,
        dedupe_key: None,
        phase: PipelinePhase::Embedding,
        status: LifecycleStatus::Running,
        severity: Severity::Info,
        visibility: Visibility::Internal,
        message: "embedding changed files".to_string(),
        timestamp: Timestamp::from(Utc::now()),
        source_id: Some(SourceId::from("src_local")),
        canonical_uri: Some("file:///workspace".to_string()),
        adapter: Some(AdapterRef {
            name: "local".to_string(),
            version: "0.1.0".to_string(),
        }),
        scope: Some(SourceScope::Directory),
        generation: Some(SourceGenerationId::from("gen_0001")),
        counts: counts.clone(),
        timing: None,
        current: Some(ProgressCurrent {
            source_item_key: Some(SourceItemKey::from("src/lib.rs")),
            document_id: None,
            chunk_id: None,
            adapter: Some("local".to_string()),
            provider: None,
            message: None,
        }),
        throughput: None,
        retry: None,
        warning: None,
        error: None,
    };
    let envelope = SuccessEnvelope {
        ok: true,
        contract_version: "2026-06-30".to_string(),
        data: SourceStatus {
            job_id,
            source_id: SourceId::from("src_local"),
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Embedding,
            heartbeat_at: Timestamp::from(Utc::now()),
            counts,
            current: event.current.clone(),
            last_error: None,
            warnings: Vec::new(),
            poll_after_ms: Some(1000),
        },
        warnings: Vec::new(),
        request_id: "req_1".to_string(),
        trace: TraceContext {
            trace_id: "trace_1".to_string(),
            span_id: Some("span_1".to_string()),
            parent_span_id: None,
            sampled: true,
            attributes: MetadataMap::default(),
        },
        pagination: None,
        job: None,
        artifacts: Vec::new(),
    };

    let value = serde_json::to_value(&event).expect("progress event");
    assert_eq!(value["phase"], "embedding");
    assert_eq!(value["current"]["source_item_key"], "src/lib.rs");
    assert_eq!(
        serde_json::from_value::<SuccessEnvelope<SourceStatus>>(
            serde_json::to_value(&envelope).unwrap()
        )
        .unwrap(),
        envelope
    );
}

#[test]
fn common_contract_enums_and_ranges_are_schema_aligned() {
    let fetch = FetchPlan {
        uri: "https://example.com".to_string(),
        method: "GET".to_string(),
        headers: RedactedHeaders {
            headers: Vec::new(),
        },
        render_required: false,
        cache_policy: CachePolicy::Revalidate,
    };
    let range = SourceRange {
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        char_start: None,
        char_end: None,
        time_start_ms: None,
        time_end_ms: None,
        dom_selector: Some("main article".to_string()),
        json_pointer: Some("/items/0".to_string()),
        yaml_path: None,
        xml_xpath: None,
        csv_row: None,
        session_turn_id: Some("turn_1".to_string()),
        turn_start: None,
        turn_end: None,
    };
    let caller = CallerContext {
        actor: Some("cli".to_string()),
        transport: TransportKind::Worker,
        scopes: vec!["axon:read".to_string()],
        visibility_ceiling: Visibility::Internal,
    };

    assert_eq!(
        serde_json::to_value(&fetch).unwrap()["cache_policy"],
        "revalidate"
    );
    assert_eq!(
        serde_json::to_value(&caller).unwrap()["transport"],
        "worker"
    );
    assert_eq!(
        serde_json::to_value(&range).unwrap()["dom_selector"],
        "main article"
    );
}

#[test]
fn watch_and_listing_dtos_are_contract_shaped() {
    let schedule = WatchSchedule {
        every_seconds: 3600,
        cron: None,
        timezone: Some("UTC".to_string()),
    };
    let request = WatchRequest {
        source: "/workspace/axon".to_string(),
        schedule: schedule.clone(),
        embed: true,
        options: AdapterOptions::default(),
        scope: Some(SourceScope::Directory),
        collection: Some("axon".to_string()),
        enabled: Some(true),
    };
    let summary = WatchSummary {
        watch_id: WatchId::from("watch_1"),
        source_id: SourceId::from("src_local"),
        enabled: true,
        schedule,
        next_run_at: Timestamp::from(Utc::now()),
        last_job_id: None,
        last_status: Some(LifecycleStatus::Completed),
    };
    let page = Page {
        items: vec![summary],
        limit: 50,
        next_cursor: None,
        total: Some(1),
    };

    let value = serde_json::to_value(&request).expect("watch request");
    assert_eq!(value["schedule"]["every_seconds"], 3600);
    assert_eq!(value["scope"], "directory");
    assert_eq!(
        serde_json::from_value::<Page<WatchSummary>>(serde_json::to_value(&page).unwrap()).unwrap(),
        page
    );
}

#[test]
fn source_job_and_watch_management_dtos_round_trip() {
    let now = Timestamp::from(Utc::now());
    let source_id = SourceId::from("src_local");
    let job_id = JobId(uuid::Uuid::new_v4());
    let counts = StageCounts {
        items_total: Some(2),
        items_done: 1,
        documents_total: Some(2),
        documents_done: 1,
        chunks_total: Some(8),
        chunks_done: 4,
        bytes_total: Some(100),
        bytes_done: 50,
    };
    let summary = SourceSummary {
        source_id: source_id.clone(),
        canonical_uri: "file:///workspace/axon".to_string(),
        display_name: "axon".to_string(),
        source_kind: SourceKind::Local,
        adapter: AdapterRef {
            name: "local".to_string(),
            version: "0.1.0".to_string(),
        },
        authority: AuthorityLevel::UserPinned,
        status: LifecycleStatus::Running,
        counts: SourceCounts {
            items_total: 2,
            items_changed: 1,
            documents_total: 1,
            chunks_total: 4,
            vector_points_total: 4,
            bytes_total: 50,
        },
        created_at: now.clone(),
        updated_at: now.clone(),
        tags: vec!["code".to_string()],
        watch_id: Some(WatchId::from("watch_1")),
        last_job_id: Some(job_id),
    };
    let detail = SourceDetail {
        summary,
        active_generation: Some(SourceGenerationId::from("gen_1")),
        latest_generation: Some(SourceGenerationId::from("gen_2")),
        items: Page {
            items: Vec::new(),
            limit: 50,
            next_cursor: None,
            total: Some(0),
        },
        documents: Page {
            items: Vec::new(),
            limit: 50,
            next_cursor: None,
            total: Some(0),
        },
        graph_refs: Vec::new(),
        metadata: MetadataMap::default(),
    };
    let job_event = JobEvent {
        event_id: "evt_1".to_string(),
        sequence: 1,
        job_id,
        attempt: 1,
        stage_id: None,
        phase: PipelinePhase::Fetching,
        status: LifecycleStatus::Running,
        severity: Severity::Info,
        visibility: Visibility::Internal,
        message: "fetching".to_string(),
        timestamp: now.clone(),
        details: MetadataMap::default(),
    };
    let job_detail = JobDetail {
        summary: JobSummary {
            job_id,
            kind: JobKind::Source,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Fetching,
            created_at: now.clone(),
            updated_at: now.clone(),
            intent: None,
            started_at: None,
            finished_at: None,
            source_id: Some(source_id),
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 1,
            priority: JobPriority::Normal,
            counts: Some(counts.clone()),
            current: None,
            heartbeat: None,
            last_error: None,
            warnings: Vec::new(),
        },
        request: None,
        progress: None,
        events: Page {
            items: vec![job_event],
            limit: 50,
            next_cursor: None,
            total: Some(1),
        },
        artifacts: Vec::new(),
        metadata: MetadataMap::default(),
    };
    let watch_control = WatchControlRequest {
        action: WatchControlAction::RunNow,
        reason: Some("manual refresh".to_string()),
        force: Some(false),
    };
    let history = WatchHistoryEntry {
        job_id,
        watch_id: WatchId::from("watch_1"),
        started_at: now.clone(),
        finished_at: None,
        status: LifecycleStatus::Running,
        counts,
        artifacts: Vec::new(),
        error: None,
    };

    assert_eq!(
        serde_json::from_value::<SourceDetail>(serde_json::to_value(&detail).unwrap()).unwrap(),
        detail
    );
    assert_eq!(
        serde_json::from_value::<JobDetail>(serde_json::to_value(&job_detail).unwrap()).unwrap(),
        job_detail
    );
    assert_eq!(
        serde_json::to_value(&watch_control).unwrap()["action"],
        "run_now"
    );
    assert_eq!(
        serde_json::from_value::<WatchHistoryEntry>(serde_json::to_value(&history).unwrap())
            .unwrap(),
        history
    );
}

#[test]
fn management_dtos_reject_unknown_fields() {
    let bad_job = serde_json::json!({
        "action": "cancel",
        "reason": "mistake",
        "force": false,
        "legacy": true
    });
    assert!(serde_json::from_value::<JobControlRequest>(bad_job).is_err());

    let bad_watch = serde_json::json!({
        "enabled": true,
        "interval_seconds": 60
    });
    assert!(serde_json::from_value::<WatchUpdateRequest>(bad_watch).is_err());
}

#[test]
fn phase_1_operation_dtos_reject_unknown_fields() {
    let upload_err = serde_json::from_value::<UploadCreateRequest>(serde_json::json!({
        "filename": "notes.md",
        "content_type": "text/markdown",
        "size_bytes": 12,
        "purpose": "source_artifact",
        "legacy": true
    }))
    .expect_err("upload request must reject unknown fields");
    assert!(
        upload_err.to_string().contains("unknown field"),
        "{upload_err}"
    );

    let watch_err = serde_json::from_value::<WatchDescriptor>(serde_json::json!({
        "watch_id": "watch_1",
        "source_id": "src_1",
        "enabled": true,
        "schedule": { "every_seconds": 3600 },
        "warnings": [],
        "legacy": true
    }))
    .expect_err("watch descriptor must reject unknown fields");
    assert!(
        watch_err.to_string().contains("unknown field"),
        "{watch_err}"
    );

    let collection_err = serde_json::from_value::<CollectionListRequest>(serde_json::json!({
        "prefix": "axon",
        "legacy": true
    }))
    .expect_err("collection list request must reject unknown fields");
    assert!(
        collection_err.to_string().contains("unknown field"),
        "{collection_err}"
    );
}
