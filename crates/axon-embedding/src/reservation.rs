//! Compatibility exports for shared provider reservations.

use axon_api::source::{JobPriority, ProviderId, ProviderKind, ReservationStateSnapshot};

pub use axon_observe::reservation::{
    ProviderReservation, ProviderReservationConfig, ProviderReservationManager,
    ProviderReservationOutcome,
};

pub type Result<T> = axon_observe::reservation::Result<T>;

#[derive(Debug, Clone)]
pub struct ProviderReservations {
    manager: ProviderReservationManager,
}

impl ProviderReservations {
    pub fn new(capacity: u32, interactive_reserve: u32) -> Self {
        Self {
            manager: ProviderReservationManager::new(ProviderReservationConfig {
                provider_id: ProviderId::new("embedding-provider-pool"),
                provider_kind: ProviderKind::Embedding,
                capacity,
                interactive_reserve,
                cooldown_after_failures: 1,
                cooldown_secs: 30,
            }),
        }
    }

    pub async fn reserve(
        &self,
        provider_id: ProviderId,
        priority: JobPriority,
        units: u32,
    ) -> Result<ProviderReservation> {
        self.manager
            .reserve_for_provider(provider_id, priority, units)
            .await
    }

    pub async fn snapshot(&self) -> ReservationStateSnapshot {
        self.manager.snapshot().await
    }
}
