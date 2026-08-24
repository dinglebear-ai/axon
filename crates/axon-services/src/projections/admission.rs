use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axon_core::config::ProjectionBatchConfig;
use axon_error::{ApiError, ErrorStage};

#[derive(Default)]
struct AdmissionState {
    global_in_flight: usize,
    callers: HashMap<String, CallerState>,
}

#[derive(Default)]
struct CallerState {
    in_flight: usize,
    starts: VecDeque<Instant>,
}

static ADMISSION_STATE: OnceLock<Mutex<AdmissionState>> = OnceLock::new();

#[derive(Debug)]
pub struct ProjectionAdmissionGuard {
    principal: String,
}

pub fn acquire_projection_admission(
    principal: &str,
    policy: &ProjectionBatchConfig,
) -> Result<ProjectionAdmissionGuard, ApiError> {
    let now = Instant::now();
    let window = Duration::from_secs(60);
    let mut state = ADMISSION_STATE
        .get_or_init(|| Mutex::new(AdmissionState::default()))
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
    })
}

impl Drop for ProjectionAdmissionGuard {
    fn drop(&mut self) {
        let Some(state) = ADMISSION_STATE.get() else {
            return;
        };
        let Ok(mut state) = state.lock() else {
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
