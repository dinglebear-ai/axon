use std::collections::BTreeMap;

use serde_json::json;

use super::*;

fn reservation_policy() -> ReservationPolicy {
    ReservationPolicy {
        supports_reservations: true,
        queue_policy: QueuePolicy::Priority,
        interactive_reserve: 1,
        cooldown_after_failures: 3,
        cooldown_secs: 90,
        retry_backoff_ms: Some(500),
    }
}

fn reservation_state() -> ReservationStateSnapshot {
    let mut priority_breakdown = BTreeMap::new();
    priority_breakdown.insert("interactive".to_string(), 1);
    priority_breakdown.insert("background".to_string(), 2);

    ReservationStateSnapshot {
        queued: 3,
        active: 2,
        available_units: 1,
        oldest_queued_ms: Some(1_250),
        priority_breakdown,
        states: vec![ReservationState::Queued, ReservationState::Active],
    }
}

fn base_provider(provider_kind: ProviderKind) -> ProviderCapability {
    ProviderCapability {
        provider_id: ProviderId::from("provider-default"),
        provider_kind,
        implementation: "test-provider".to_string(),
        version: "1.2.3".to_string(),
        health: HealthStatus::Degraded,
        limits: ProviderLimits {
            max_concurrency: Some(8),
            max_batch_size: Some(32),
            max_input_bytes: Some(4 * 1024 * 1024),
            timeout_ms: Some(30_000),
            rate_limit_per_minute: Some(120),
            max_queue_depth: Some(512),
            interactive_reserved_concurrency: Some(1),
            background_max_concurrency: Some(6),
            maintenance_max_concurrency: Some(1),
        },
        features: vec!["reservations".to_string(), "cooldown".to_string()],
        cooldown_until: Some(Timestamp("2026-06-30T12:00:00Z".to_string())),
        last_error: None,
        reservation_policy: reservation_policy(),
        reservation_state: reservation_state(),
        cost_class: ProviderCostClass::Internal,
        degraded_modes: vec![DegradedMode::LowerConcurrency],
        fake_overrides_supported: true,
        embedding: None,
        llm: None,
        vector_store: None,
        fetch: None,
        render: None,
        credential: None,
    }
}

#[test]
fn provider_capability_serializes_common_contract_fields() {
    let provider = base_provider(ProviderKind::Embedding);
    let value = serde_json::to_value(provider).expect("provider capability");

    assert_eq!(value["provider_id"], "provider-default");
    assert_eq!(value["provider_kind"], "embedding");
    assert_eq!(value["implementation"], "test-provider");
    assert_eq!(value["limits"]["max_concurrency"], 8);
    assert_eq!(value["cooldown_until"], "2026-06-30T12:00:00Z");
    assert_eq!(value["reservation_policy"]["queue_policy"], "priority");
    assert_eq!(value["reservation_state"]["states"][0], "queued");
    assert_eq!(
        value["reservation_state"]["priority_breakdown"]["interactive"],
        1
    );
    assert_eq!(value["cost_class"], "internal");
    assert_eq!(value["degraded_modes"][0], "lower_concurrency");
    assert_eq!(value["fake_overrides_supported"], true);
}

#[test]
fn embedding_capability_serializes_family_specific_contract_fields() {
    let mut provider = base_provider(ProviderKind::Embedding);
    provider.embedding = Some(EmbeddingProviderCapability {
        model_id: "Qwen/Qwen3-Embedding-0.6B".to_string(),
        dimensions: 1024,
        max_input_tokens: 32_768,
        max_batch_tokens: 262_144,
        instruction_support: InstructionSupport::QueryAndDocument,
        sparse_output: false,
        batch_limits: BatchLimits {
            max_items: 32,
            max_tokens: 262_144,
            max_bytes: Some(4 * 1024 * 1024),
        },
    });

    let value = serde_json::to_value(provider).expect("embedding capability");

    assert_eq!(value["embedding"]["model_id"], "Qwen/Qwen3-Embedding-0.6B");
    assert_eq!(value["embedding"]["dimensions"], 1024);
    assert_eq!(
        value["embedding"]["instruction_support"],
        "query_and_document"
    );
    assert_eq!(value["embedding"]["batch_limits"]["max_items"], 32);
}

#[test]
fn vector_and_llm_capabilities_have_typed_family_sections() {
    let mut vector = base_provider(ProviderKind::Vector);
    vector.vector_store = Some(VectorStoreCapability {
        dense: true,
        sparse: true,
        hybrid: true,
        payload_filters: true,
        payload_indexes: vec!["seed_url".to_string(), "source_type".to_string()],
        delete_by_filter: true,
        generation_publish: true,
        collection_aliases: true,
        consistency: VectorConsistency::Tunable,
    });

    let mut llm = base_provider(ProviderKind::Llm);
    llm.llm = Some(LlmProviderCapability {
        model_id: "gemini-2.5-pro".to_string(),
        context_window: 1_000_000,
        streaming: true,
        json_schema: true,
        tool_use: true,
        structured_output: true,
        max_output_tokens: 65_536,
    });

    let vector_value = serde_json::to_value(vector).expect("vector capability");
    let llm_value = serde_json::to_value(llm).expect("llm capability");

    assert_eq!(vector_value["vector_store"]["hybrid"], true);
    assert_eq!(vector_value["vector_store"]["generation_publish"], true);
    assert_eq!(vector_value["vector_store"]["consistency"], "tunable");
    assert_eq!(llm_value["llm"]["context_window"], 1_000_000);
    assert_eq!(llm_value["llm"]["json_schema"], true);
}

#[test]
fn fetch_render_and_credential_capabilities_have_typed_family_sections() {
    let mut fetch = base_provider(ProviderKind::Fetch);
    fetch.fetch = Some(FetchProviderCapability {
        schemes: vec!["http".to_string(), "https".to_string()],
        redirect_policy: RedirectPolicy::SameSite,
        header_policy: HeaderPolicy::RedactedPassthrough,
    });

    let mut render = base_provider(ProviderKind::Render);
    render.render = Some(RenderProviderCapability {
        render_modes: vec![RenderMode::Http, RenderMode::Chrome, RenderMode::AutoSwitch],
        browser_pool_limits: BrowserPoolLimits {
            max_browsers: 2,
            max_pages_per_browser: 8,
            max_page_lifetime_ms: 120_000,
        },
        script_support: true,
    });

    let mut credential = base_provider(ProviderKind::Credential);
    credential.credential = Some(CredentialProviderCapability {
        auth_schemes: vec!["bearer".to_string(), "oauth".to_string()],
        redaction_policy: RedactionPolicy::Strict,
    });

    let fetch_value = serde_json::to_value(fetch).expect("fetch capability");
    let render_value = serde_json::to_value(render).expect("render capability");
    let credential_value = serde_json::to_value(credential).expect("credential capability");

    assert_eq!(fetch_value["fetch"]["redirect_policy"], "same_site");
    assert_eq!(render_value["render"]["render_modes"][2], "auto_switch");
    assert_eq!(credential_value["credential"]["redaction_policy"], "strict");
}

#[test]
fn provider_capability_rejects_unknown_fields() {
    let mut value = serde_json::to_value(base_provider(ProviderKind::Embedding)).unwrap();
    value["unexpected"] = json!(true);

    let err = serde_json::from_value::<ProviderCapability>(value).unwrap_err();
    assert!(err.to_string().contains("unknown field"));
}
