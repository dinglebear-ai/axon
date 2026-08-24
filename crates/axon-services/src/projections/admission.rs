use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axon_core::config::ProjectionBatchConfig;
use axon_error::{ApiError, ErrorStage};

#[derive(Debug, Default)]
struct AdmissionState {
    global_in_flight: usize,
    callers: HashMap<String, CallerState>,
}

#[derive(Debug, Default)]
struct CallerState {
    in_flight: usize,
    starts: VecDeque<Instant>,
}

static ADMISSION_LIMITER: OnceLock<ProjectionAdmissionLimiter> = OnceLock::new();

#[derive(Clone, Default)]
struct ProjectionAdmissionLimiter {
    state: Arc<Mutex<AdmissionState>>,
}

#[derive(Debug)]
pub struct ProjectionAdmissionGuard {
    principal: String,
    state: Arc<Mutex<AdmissionState>>,
}

pub fn acquire_projection_admission(
    principal: &str,
    policy: &ProjectionBatchConfig,
) -> Result<ProjectionAdmissionGuard, ApiError> {
    ADMISSION_LIMITER
        .get_or_init(ProjectionAdmissionLimiter::default)
        .acquire(principal, policy, Instant::now())
}

impl ProjectionAdmissionLimiter {
    fn acquire(
        &self,
        principal: &str,
        policy: &ProjectionBatchConfig,
        now: Instant,
    ) -> Result<ProjectionAdmissionGuard, ApiError> {
        let window = Duration::from_secs(60);
        let mut state = self
            .state
            .lock()
            .map_err(|_| admission_error("projection.admission_state_unavailable"))?;
        if state.global_in_flight >= policy.global_max_in_flight_requests {
            return Err(admission_error("projection.global_admission_saturated"));
        }
        let caller = state.callers.entry(principal.to_string()).or_default();
        while caller
            .starts
            .front()
            .is_some_and(|started| now.duration_since(*started) >= window)
        {
            caller.starts.pop_front();
        }
        if caller.in_flight >= policy.caller_max_in_flight_requests {
            return Err(admission_error("projection.caller_admission_saturated"));
        }
        if caller.starts.len() as u64 >= policy.caller_rate_limit_per_minute {
            return Err(admission_error("projection.caller_rate_limited"));
        }
        caller.in_flight += 1;
        caller.starts.push_back(now);
        state.global_in_flight += 1;
        Ok(ProjectionAdmissionGuard {
            principal: principal.to_string(),
            state: Arc::clone(&self.state),
        })
    }
}

impl Drop for ProjectionAdmissionGuard {
    fn drop(&mut self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.global_in_flight = state.global_in_flight.saturating_sub(1);
        if let Some(caller) = state.callers.get_mut(&self.principal) {
            caller.in_flight = caller.in_flight.saturating_sub(1);
        }
    }
}

fn admission_error(code: &str) -> ApiError {
    ApiError::new(
        code,
        ErrorStage::Leasing,
        "projection admission limit exceeded",
    )
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
