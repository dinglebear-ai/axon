use std::sync::{Mutex, OnceLock};

use axon_api::source::{ApiError, ErrorStage, JobId};

static FAIL_JOB: OnceLock<Mutex<Option<JobId>>> = OnceLock::new();

pub(crate) fn install_precommit_failure(job_id: JobId) {
    *FAIL_JOB
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("hook lock") = Some(job_id);
}

pub(super) fn fail_before_commit(attempts: &[(JobId, u32)]) -> crate::boundary::Result<()> {
    let mut hook = FAIL_JOB
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("hook lock");
    if hook.is_some_and(|job_id| attempts.iter().any(|(candidate, _)| *candidate == job_id)) {
        hook.take();
        return Err(ApiError::new(
            "job_recovery.precommit_failed",
            ErrorStage::Storage,
            "injected recovery pre-commit failure",
        ));
    }
    Ok(())
}
