//! Shared provider reservation, fairness, and cooldown state.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;

use axon_api::source::{
    ApiError, HealthStatus, JobId, JobPriority, ProviderCoolingSnapshot, ProviderId, ProviderKind,
    ProviderReservationSnapshot, ProviderReservationStatus, ReservationId, ReservationState,
    ReservationStateSnapshot, StageId, Timestamp,
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
    next_reservation_sequence: u64,
    active: u32,
    active_by_priority: BTreeMap<String, u32>,
    consecutive_failures: u32,
    health: HealthStatus,
    cooldown_started_at: Option<Timestamp>,
    cooldown_until: Option<Timestamp>,
    cooldown_deadline: Option<DateTime<Utc>>,
    last_error_code: Option<String>,
}

#[derive(Debug)]
pub struct ProviderReservation {
    reservation_id: ReservationId,
    job_id: Option<JobId>,
    stage_id: Option<StageId>,
    provider_id: ProviderId,
    provider_kind: ProviderKind,
    priority: JobPriority,
    requested_units: u32,
    granted_units: u32,
    acquired_at: Timestamp,
    expires_at: Option<Timestamp>,
    state: Arc<Mutex<ReservationStateInner>>,
    released: bool,
}

#[derive(Debug, Clone)]
pub struct ProviderReservationContext {
    pub job_id: JobId,
    pub stage_id: Option<StageId>,
    pub provider_id: Option<ProviderId>,
    pub priority: JobPriority,
    pub units: u32,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderReservationOutcome {
    Recorded,
    Cooling,
}

/// A provider failure after reservation state and retry metadata are applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedProviderFailure {
    pub error: ApiError,
    pub outcome: ProviderReservationOutcome,
}

impl ProviderReservationManager {
    pub fn new(config: ProviderReservationConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(ReservationStateInner {
                config,
                next_reservation_sequence: 0,
                active: 0,
                active_by_priority: BTreeMap::new(),
                consecutive_failures: 0,
                health: HealthStatus::Healthy,
                cooldown_started_at: None,
                cooldown_until: None,
                cooldown_deadline: None,
                last_error_code: None,
            })),
        }
    }

    pub async fn reserve(&self, priority: JobPriority, units: u32) -> Result<ProviderReservation> {
        self.reserve_inner(None, None, None, priority, units, None)
            .await
    }

    pub async fn reserve_for_provider(
        &self,
        provider_id: ProviderId,
        priority: JobPriority,
        units: u32,
    ) -> Result<ProviderReservation> {
        self.reserve_inner(Some(provider_id), None, None, priority, units, None)
            .await
    }

    pub async fn reserve_with_context(
        &self,
        context: ProviderReservationContext,
    ) -> Result<ProviderReservation> {
        self.reserve_inner(
            context.provider_id,
            Some(context.job_id),
            context.stage_id,
            context.priority,
            context.units,
            context.ttl_seconds,
        )
        .await
    }

    async fn reserve_inner(
        &self,
        provider_id: Option<ProviderId>,
        job_id: Option<JobId>,
        stage_id: Option<StageId>,
        priority: JobPriority,
        units: u32,
        ttl_seconds: Option<u64>,
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
        state.next_reservation_sequence += 1;
        let acquired_at = Utc::now();
        let expires_at =
            ttl_seconds.map(|ttl| Timestamp::from(acquired_at + Duration::seconds(ttl as i64)));

        Ok(ProviderReservation {
            reservation_id: ReservationId::from(format!("res_{}", state.next_reservation_sequence)),
            job_id,
            stage_id,
            provider_id: effective_provider_id,
            provider_kind: state.config.provider_kind,
            priority,
            requested_units: units,
            granted_units: units,
            acquired_at: Timestamp::from(acquired_at),
            expires_at,
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
        state.record_failure(code.into(), retryable)
    }

    /// Record a typed provider failure and return the same error enriched with
    /// the provider id and any newly-active cooling window.
    pub async fn record_api_failure(&self, mut error: ApiError) -> RecordedProviderFailure {
        let mut state = self.state.lock().expect("reservation state mutex poisoned");
        if error.provider_id.is_none() {
            error.provider_id = Some(state.config.provider_id.0.clone());
        }
        let outcome = state.record_failure(error.code.to_string(), error.retryable);
        if outcome == ProviderReservationOutcome::Cooling {
            error.cooldown_until = state.cooldown_deadline;
            error.retry_after_ms = Some(state.config.cooldown_secs.saturating_mul(1_000));
            error
                .details
                .insert("cooling_reason".to_string(), error.code.to_string());
        }

        RecordedProviderFailure { error, outcome }
    }

    pub async fn record_success(&self) {
        let mut state = self.state.lock().expect("reservation state mutex poisoned");
        state.consecutive_failures = 0;
        state.health = HealthStatus::Healthy;
        state.cooldown_started_at = None;
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

    pub async fn cooling_snapshot(&self) -> Option<ProviderCoolingSnapshot> {
        let mut state = self.state.lock().expect("reservation state mutex poisoned");
        state.refresh_cooldown();
        if state.health != HealthStatus::Cooling {
            return None;
        }
        Some(ProviderCoolingSnapshot {
            reason: state
                .last_error_code
                .clone()
                .unwrap_or_else(|| "provider.cooling".to_string()),
            started_at: state
                .cooldown_started_at
                .clone()
                .unwrap_or_else(|| Timestamp::from(Utc::now())),
            retry_after: state.cooldown_until.clone(),
            degraded: true,
        })
    }
}

impl ProviderReservation {
    pub fn reservation_id(&self) -> &ReservationId {
        &self.reservation_id
    }

    pub fn job_id(&self) -> Option<JobId> {
        self.job_id
    }

    pub fn stage_id(&self) -> Option<StageId> {
        self.stage_id
    }

    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    pub fn provider_kind(&self) -> ProviderKind {
        self.provider_kind
    }

    pub fn priority(&self) -> JobPriority {
        self.priority
    }

    pub fn snapshot(&self) -> ProviderReservationSnapshot {
        ProviderReservationSnapshot {
            reservation_id: self.reservation_id.clone(),
            provider_kind: self.provider_kind,
            provider_id: Some(self.provider_id.clone()),
            priority: self.priority,
            requested_units: self.requested_units,
            granted_units: self.granted_units,
            acquired_at: Some(self.acquired_at.clone()),
            expires_at: self.expires_at.clone(),
            status: ProviderReservationStatus::Active,
            queue_depth: None,
            cooling: None,
        }
    }
}

impl Drop for ProviderReservation {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        let mut state = self.state.lock().expect("reservation state mutex poisoned");
        state.active = state.active.saturating_sub(self.granted_units);
        let priority = priority_key(self.priority);
        if let Some(count) = state.active_by_priority.get_mut(&priority) {
            *count = count.saturating_sub(self.granted_units);
            if *count == 0 {
                state.active_by_priority.remove(&priority);
            }
        }
        self.released = true;
    }
}

impl ReservationStateInner {
    fn record_failure(&mut self, code: String, retryable: bool) -> ProviderReservationOutcome {
        self.last_error_code = Some(code);
        if !retryable {
            self.health = HealthStatus::Unavailable;
            self.cooldown_started_at = None;
            self.cooldown_until = None;
            self.cooldown_deadline = None;
            return ProviderReservationOutcome::Recorded;
        }

        self.consecutive_failures += 1;
        if self.consecutive_failures < self.config.cooldown_after_failures {
            self.health = HealthStatus::Degraded;
            return ProviderReservationOutcome::Recorded;
        }

        self.health = HealthStatus::Cooling;
        let now = Utc::now();
        if self.cooldown_started_at.is_none() {
            self.cooldown_started_at = Some(Timestamp::from(now));
        }
        let deadline = now + Duration::seconds(self.config.cooldown_secs as i64);
        self.cooldown_until = Some(Timestamp::from(deadline));
        self.cooldown_deadline = Some(deadline);
        ProviderReservationOutcome::Cooling
    }

    fn refresh_cooldown(&mut self) {
        if self
            .cooldown_deadline
            .is_some_and(|deadline| deadline <= Utc::now())
        {
            self.consecutive_failures = 0;
            self.health = HealthStatus::Degraded;
            self.cooldown_started_at = None;
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
