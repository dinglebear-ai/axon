//! Shared provider reservation, fairness, and cooldown state.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;

use axon_api::source::{
    ApiError, HealthStatus, JobPriority, ProviderId, ProviderKind, ReservationState,
    ReservationStateSnapshot, Timestamp,
};
use axon_error::ErrorStage;
use chrono::{DateTime, Duration, Utc};

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug, Clone)]
pub struct ProviderReservationConfig {
    pub provider_id: ProviderId,
    pub provider_kind: ProviderKind,
    pub capacity: u32,
    pub interactive_reserve: u32,
    pub cooldown_after_failures: u32,
    pub cooldown_secs: u64,
}

#[derive(Debug, Clone)]
pub struct ProviderReservationManager {
    state: Arc<Mutex<ReservationStateInner>>,
}

#[derive(Debug)]
struct ReservationStateInner {
    config: ProviderReservationConfig,
    active: u32,
    active_by_priority: BTreeMap<String, u32>,
    consecutive_failures: u32,
    health: HealthStatus,
    cooldown_until: Option<Timestamp>,
    cooldown_deadline: Option<DateTime<Utc>>,
    last_error_code: Option<String>,
}

#[derive(Debug)]
pub struct ProviderReservation {
    provider_id: ProviderId,
    provider_kind: ProviderKind,
    priority: JobPriority,
    units: u32,
    state: Arc<Mutex<ReservationStateInner>>,
    released: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderReservationOutcome {
    Recorded,
    Cooling,
}

impl ProviderReservationManager {
    pub fn new(config: ProviderReservationConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(ReservationStateInner {
                config,
                active: 0,
                active_by_priority: BTreeMap::new(),
                consecutive_failures: 0,
                health: HealthStatus::Healthy,
                cooldown_until: None,
                cooldown_deadline: None,
                last_error_code: None,
            })),
        }
    }

    pub async fn reserve(&self, priority: JobPriority, units: u32) -> Result<ProviderReservation> {
        self.reserve_inner(None, priority, units).await
    }

    pub async fn reserve_for_provider(
        &self,
        provider_id: ProviderId,
        priority: JobPriority,
        units: u32,
    ) -> Result<ProviderReservation> {
        self.reserve_inner(Some(provider_id), priority, units).await
    }

    async fn reserve_inner(
        &self,
        provider_id: Option<ProviderId>,
        priority: JobPriority,
        units: u32,
    ) -> Result<ProviderReservation> {
        let mut state = self.state.lock().expect("reservation state mutex poisoned");
        let effective_provider_id = provider_id.unwrap_or_else(|| state.config.provider_id.clone());
        state.refresh_cooldown();
        if state.health == HealthStatus::Unavailable {
            let code = state
                .last_error_code
                .as_deref()
                .unwrap_or("provider.unavailable");
            let mut error =
                state.error_for(&effective_provider_id, code, "provider is unavailable");
            error.retryable = false;
            return Err(error);
        }
        if state.cooldown_until.is_some() {
            return Err(state.error_for(
                &effective_provider_id,
                "provider.cooling",
                "provider is cooling down",
            ));
        }
        if units == 0 {
            return Err(state.error_for(
                &effective_provider_id,
                "provider.invalid_reservation",
                "reservation units must be > 0",
            ));
        }

        let available = state.config.capacity.saturating_sub(state.active);
        if available < units || !state.preserves_interactive_capacity(priority, units) {
            return Err(state.error_for(
                &effective_provider_id,
                "provider.capacity_exhausted",
                "provider reservation capacity exhausted",
            ));
        }

        state.active += units;
        *state
            .active_by_priority
            .entry(priority_key(priority))
            .or_default() += units;

        Ok(ProviderReservation {
            provider_id: effective_provider_id,
            provider_kind: state.config.provider_kind,
            priority,
            units,
            state: Arc::clone(&self.state),
            released: false,
        })
    }

    pub async fn snapshot(&self) -> ReservationStateSnapshot {
        let state = self.state.lock().expect("reservation state mutex poisoned");
        let priority_breakdown = state
            .active_by_priority
            .iter()
            .map(|(priority, count)| (priority.clone(), *count))
            .collect();
        ReservationStateSnapshot {
            queued: 0,
            active: state.active,
            available_units: state.config.capacity.saturating_sub(state.active),
            oldest_queued_ms: None,
            priority_breakdown,
            states: if state.active == 0 {
                Vec::new()
            } else {
                vec![ReservationState::Active]
            },
        }
    }

    pub async fn record_failure(
        &self,
        code: impl Into<String>,
        retryable: bool,
    ) -> ProviderReservationOutcome {
        let mut state = self.state.lock().expect("reservation state mutex poisoned");
        state.last_error_code = Some(code.into());
        if !retryable {
            state.health = HealthStatus::Unavailable;
            state.cooldown_until = None;
            state.cooldown_deadline = None;
            return ProviderReservationOutcome::Recorded;
        }

        state.consecutive_failures += 1;
        if state.consecutive_failures >= state.config.cooldown_after_failures {
            state.health = HealthStatus::Cooling;
            let deadline = Utc::now() + Duration::seconds(state.config.cooldown_secs as i64);
            state.cooldown_until = Some(Timestamp::from(deadline));
            state.cooldown_deadline = Some(deadline);
            ProviderReservationOutcome::Cooling
        } else {
            state.health = HealthStatus::Degraded;
            ProviderReservationOutcome::Recorded
        }
    }

    pub async fn record_success(&self) {
        let mut state = self.state.lock().expect("reservation state mutex poisoned");
        state.consecutive_failures = 0;
        state.health = HealthStatus::Healthy;
        state.cooldown_until = None;
        state.cooldown_deadline = None;
        state.last_error_code = None;
    }

    pub async fn health(&self) -> HealthStatus {
        let mut state = self.state.lock().expect("reservation state mutex poisoned");
        state.refresh_cooldown();
        state.health
    }

    pub async fn cooldown_until(&self) -> Option<Timestamp> {
        let mut state = self.state.lock().expect("reservation state mutex poisoned");
        state.refresh_cooldown();
        state.cooldown_until.clone()
    }
}

impl ProviderReservation {
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    pub fn provider_kind(&self) -> ProviderKind {
        self.provider_kind
    }

    pub fn priority(&self) -> JobPriority {
        self.priority
    }
}

impl Drop for ProviderReservation {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        let mut state = self.state.lock().expect("reservation state mutex poisoned");
        state.active = state.active.saturating_sub(self.units);
        let priority = priority_key(self.priority);
        if let Some(count) = state.active_by_priority.get_mut(&priority) {
            *count = count.saturating_sub(self.units);
            if *count == 0 {
                state.active_by_priority.remove(&priority);
            }
        }
        self.released = true;
    }
}

impl ReservationStateInner {
    fn refresh_cooldown(&mut self) {
        if self
            .cooldown_deadline
            .is_some_and(|deadline| deadline <= Utc::now())
        {
            self.consecutive_failures = 0;
            self.health = HealthStatus::Degraded;
            self.cooldown_until = None;
            self.cooldown_deadline = None;
        }
    }

    fn preserves_interactive_capacity(&self, priority: JobPriority, units: u32) -> bool {
        if !matches!(priority, JobPriority::Background | JobPriority::Maintenance) {
            return true;
        }
        let available_after = self
            .config
            .capacity
            .saturating_sub(self.active)
            .saturating_sub(units);
        available_after >= self.config.interactive_reserve
    }

    fn error_for(&self, provider_id: &ProviderId, code: &str, message: &str) -> ApiError {
        ApiError::new(code, ErrorStage::Leasing, message).with_provider_id(provider_id.0.clone())
    }
}

fn priority_key(priority: JobPriority) -> String {
    match priority {
        JobPriority::Interactive => "interactive",
        JobPriority::High => "high",
        JobPriority::Normal => "normal",
        JobPriority::Background => "background",
        JobPriority::Maintenance => "maintenance",
    }
    .to_string()
}
