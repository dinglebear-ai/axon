use axon_api::source::*;

use super::{FakeVectorStore, Result};

impl FakeVectorStore {
    pub(super) async fn capabilities_inner(&self) -> Result<ProviderCapability> {
        let state = self.capability_state();
        let store_state = self.state.lock().await;
        let sparse_configured = store_state
            .collections
            .values()
            .any(|spec| spec.sparse.is_some());
        let payload_indexes = store_state
            .collections
            .values()
            .next()
            .map(|spec| {
                spec.payload_indexes
                    .iter()
                    .map(|index| index.field_name.clone())
                    .collect()
            })
            .unwrap_or_default();
        drop(store_state);
        Ok(ProviderCapability {
            provider_id: self.provider_id.clone(),
            provider_kind: ProviderKind::Vector,
            implementation: "fake".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            health: state.health,
            limits: ProviderLimits {
                max_concurrency: Some(2),
                interactive_reserved_concurrency: Some(1),
                background_max_concurrency: Some(1),
                ..ProviderLimits::default()
            },
            features: vec!["dense".to_string(), "delete_by_chunk".to_string()],
            cooldown_until: state.cooldown_until,
            last_error: state.last_error,
            reservation_policy: ReservationPolicy {
                supports_reservations: true,
                queue_policy: QueuePolicy::Priority,
                interactive_reserve: 1,
                cooldown_after_failures: 1,
                cooldown_secs: 30,
                retry_backoff_ms: Some(100),
            },
            reservation_state: ReservationStateSnapshot {
                queued: 0,
                active: 0,
                available_units: 2,
                oldest_queued_ms: None,
                priority_breakdown: Default::default(),
                states: vec![ReservationState::Granted],
            },
            cost_class: ProviderCostClass::Internal,
            degraded_modes: Vec::new(),
            fake_overrides_supported: true,
            embedding: None,
            llm: None,
            vector_store: Some(VectorStoreCapability {
                dense: true,
                sparse: sparse_configured,
                hybrid: sparse_configured,
                payload_filters: true,
                payload_indexes,
                delete_by_filter: true,
                generation_publish: true,
                collection_aliases: false,
                consistency: VectorConsistency::Strong,
            }),
            fetch: None,
            render: None,
            credential: None,
        })
    }
}
