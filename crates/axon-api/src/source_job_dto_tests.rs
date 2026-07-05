use super::*;

#[test]
fn job_create_request_serializes_required_contract_fields() {
    let request = JobCreateRequest {
        request_id: Some("req_contract".to_string()),
        job_kind: JobKind::Source,
        job_intent: JobIntent::Run,
        source_id: None,
        watch_id: None,
        parent_job_id: Some(uuid::Uuid::from_u128(1).into()),
        root_job_id: Some(uuid::Uuid::from_u128(1).into()),
        attempt: 1,
        priority: JobPriority::Normal,
        idempotency_key: None,
        stage_plan: vec![JobStagePlan {
            phase: PipelinePhase::Fetching,
            required: true,
            provider_requirements: Vec::new(),
            estimated_items: Some(1),
        }],
        request: Some(serde_json::json!({"source": "https://example.com"})),
        auth_snapshot: AuthSnapshot::default(),
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_contract")),
        requirements: {
            let mut requirements = MetadataMap::new();
            requirements.insert("provider".to_string(), serde_json::json!("web"));
            requirements
        },
        result_schema: Some("source_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
    };

    let json = serde_json::to_value(&request).expect("serialize request");
    assert!(json.get("auth_snapshot").is_some());
    assert!(json.get("config_snapshot_id").is_some());
    assert!(json.get("stage_plan").is_some());
    assert!(json.get("requirements").is_some());
    assert!(json.get("result_schema").is_some());
    assert!(json.get("parent_job_id").is_some());
    assert!(json.get("root_job_id").is_some());
    assert!(json.get("attempt").is_some());
    assert!(json.get("warnings").is_some());
    assert!(json.get("error").is_some());

    let round_trip: JobCreateRequest = serde_json::from_value(json).expect("deserialize request");
    assert_eq!(round_trip, request);
}
