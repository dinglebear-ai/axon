//! In-memory provider reservation model used by tests and local orchestration.

use std::sync::Arc;

use axon_api::source::*;
use tokio::sync::Mutex;

use crate::provider::Result;

#[derive(Debug, Clone)]
pub struct ProviderReservations {
    state: Arc<Mutex<ReservationStateInner>>,
}

#[derive(Debug)]
struct ReservationStateInner {
    capacity: u32,
    interactive_reserve: u32,
    active: u32,
    background_active: u32,
}

#[derive(Debug)]
pub struct ProviderReservation {
    provider_id: ProviderId,
    priority: JobPriority,
    units: u32,
    state: Arc<Mutex<ReservationStateInner>>,
    released: bool,
}

impl ProviderReservations {
    pub fn new(capacity: u32, interactive_reserve: u32) -> Self {
        Self {
            state: Arc::new(Mutex::new(ReservationStateInner {
                capacity,
                interactive_reserve,
                active: 0,
                background_active: 0,
            })),
        }
    }

    pub async fn reserve(
        &self,
        provider_id: ProviderId,
        priority: JobPriority,
        units: u32,
    ) -> Result<ProviderReservation> {
        let mut state = self.state.lock().await;
        let background_like =
            matches!(priority, JobPriority::Background | JobPriority::Maintenance);
        let reserved_for_interactive = if background_like {
            state.interactive_reserve
        } else {
            0
        };
        let available = state.capacity.saturating_sub(state.active);
        if units == 0
            || available < units
            || (background_like && available <= reserved_for_interactive)
        {
            return Err(ApiError::new(
                "provider.capacity_exhausted",
                axon_error::ErrorStage::Embedding,
                "provider reservation capacity exhausted",
            )
            .with_provider_id(provider_id.0.clone()));
        }

        state.active += units;
        if background_like {
            state.background_active += units;
        }

        Ok(ProviderReservation {
            provider_id,
            priority,
            units,
            state: Arc::clone(&self.state),
            released: false,
        })
    }

    pub async fn snapshot(&self) -> ReservationStateSnapshot {
        let state = self.state.lock().await;
        ReservationStateSnapshot {
            queued: 0,
            active: state.active,
            available_units: state.capacity.saturating_sub(state.active),
            oldest_queued_ms: None,
            priority_breakdown: Default::default(),
            states: vec![ReservationState::Active],
        }
    }
}

impl ProviderReservation {
    pub fn priority(&self) -> JobPriority {
        self.priority
    }

    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }
}

impl Drop for ProviderReservation {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        if let Ok(mut state) = self.state.try_lock() {
            state.active = state.active.saturating_sub(self.units);
            if matches!(
                self.priority,
                JobPriority::Background | JobPriority::Maintenance
            ) {
                state.background_active = state.background_active.saturating_sub(self.units);
            }
            self.released = true;
        }
    }
}
