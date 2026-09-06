use axon_api::source::*;

use super::{
    HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES, HEALTH_TRACKER_COOLDOWN_SECS, QdrantVectorStore,
};

pub(crate) async fn snapshot(store: &QdrantVectorStore) -> ProviderCapability {
    let (probed, probe_error) = probe_health(store).await;
    let tracked = store.health.health().await;
    let cooldown_until = store.health.cooldown_until().await;
    let (health, last_error) =
        if matches!(tracked, HealthStatus::Cooling | HealthStatus::Unavailable) {
            (
                tracked,
                store
                    .health
                    .cooling_snapshot()
                    .await
                    .map(|cooling| {
                        ApiError::new("provider.cooling", ErrorStage::Observing, cooling.reason)
                            .with_provider_id(store.provider_id().0.clone())
                    })
                    .or(probe_error),
            )
        } else {
            (probed, probe_error)
        };
    ProviderCapability {
        provider_id: store.provider_id().clone(),
        provider_kind: ProviderKind::Vector,
        implementation: "qdrant".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        health,
        limits: ProviderLimits::default(),
        features: [
            "dense",
            "sparse",
            "hybrid",
            "payload_filters",
            "payload_indexes",
            "generation_publish",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        cooldown_until,
        last_error,
        reservation_policy: ReservationPolicy {
            supports_reservations: false,
            queue_policy: QueuePolicy::Fifo,
            interactive_reserve: 0,
            cooldown_after_failures: HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES,
            cooldown_secs: HEALTH_TRACKER_COOLDOWN_SECS,
            retry_backoff_ms: None,
        },
        reservation_state: ReservationStateSnapshot {
            queued: 0,
            active: 0,
            available_units: 0,
            oldest_queued_ms: None,
            priority_breakdown: Default::default(),
            states: Vec::new(),
        },
        cost_class: ProviderCostClass::Internal,
        degraded_modes: Vec::new(),
        fake_overrides_supported: false,
        embedding: None,
        llm: None,
        vector_store: Some(VectorStoreCapability {
            dense: true,
            sparse: true,
            hybrid: true,
            payload_filters: true,
            payload_indexes: Vec::new(),
            delete_by_filter: true,
            generation_publish: true,
            collection_aliases: true,
            consistency: VectorConsistency::Strong,
        }),
        fetch: None,
        render: None,
        credential: None,
    }
}

async fn probe_health(store: &QdrantVectorStore) -> (HealthStatus, Option<ApiError>) {
    let http = match store.http() {
        Ok(http) => http,
        Err(error) => return (HealthStatus::Unavailable, Some(error)),
    };
    let url = format!("{}/", http.endpoint().root());
    match http
        .get_json(ErrorStage::Observing, &url, "qdrant_health")
        .await
    {
        Ok(_) => (HealthStatus::Healthy, None),
        Err(error) => (HealthStatus::Unavailable, Some(error)),
    }
}
