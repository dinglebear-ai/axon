//! Real execution for a claimed `JobKind::Extract` unified job.
//!
//! `claimed.request_json` carries `{"urls": [...], "config_json": "..."}` —
//! the same `config_json` snapshot shape the legacy extract runner decodes
//! via [`apply_config_snapshot`], reused here so both paths apply identical
//! config precedence. `axon_extract::sync::extract_sync` is a pure domain
//! call with no `ServiceContext` dependency, so calling it directly from
//! `axon-jobs` does not cross the `axon-jobs -> axon-services` layering
//! boundary that blocks other job kinds (e.g. `Source`) from being wired
//! here yet.

use super::*;

pub(super) async fn run_extract_claimed(
    pool: &SqlitePool,
    cfg: &Config,
    store: &SqliteUnifiedJobStore,
    claimed: &UnifiedClaimedJob,
    shutdown: &CancellationToken,
) {
    let request = match extract_request_from_json(claimed.request_json.as_ref()) {
        Ok(request) => request,
        Err(error) => {
            fail_unified_claimed(pool, store, claimed, error).await;
            return;
        }
    };

    let mut effective_cfg = match apply_config_snapshot(cfg, &request.config_json) {
        Ok(cfg) => cfg,
        Err(error) => {
            fail_unified_claimed(
                pool,
                store,
                claimed,
                ApiError::new(
                    "job_runner.invalid_config_snapshot",
                    ErrorStage::Planning,
                    error.to_string(),
                ),
            )
            .await;
            return;
        }
    };
    effective_cfg.output_dir = effective_cfg
        .output_dir
        .join("extract-jobs")
        .join(claimed.job_id.0.to_string());
    effective_cfg.output_path = None;

    if let Err(error) = heartbeat(store, claimed, PipelinePhase::Parsing).await {
        tracing::warn!(job_id = %claimed.job_id.0, error = %error.message, "unified worker extract heartbeat failed");
    }

    let prompt = effective_cfg.query.clone().unwrap_or_default();
    // Map the error to `String` *inside* the future (not after `select!`
    // resolves it) — `tokio::select!` builds one combined output type across
    // all branches, so the raw `Box<dyn Error>` (not `Send`) would make that
    // combined type non-`Send` regardless of what the outer match does with it.
    let extract_fut = futures::FutureExt::map(
        axon_extract::sync::extract_sync(&effective_cfg, &request.urls, &prompt),
        |result: Result<_, Box<dyn std::error::Error>>| result.map_err(|e| e.to_string()),
    );
    let result: Result<_, String> = tokio::select! {
        _ = shutdown.cancelled() => {
            mark_canceled(pool, store, claimed).await;
            return;
        }
        result = extract_fut => result,
    };

    match result {
        Ok(_summary) => {
            if let Err(error) = mark_terminal(
                pool,
                claimed,
                LifecycleStatus::Completed,
                PipelinePhase::Complete,
                None,
            )
            .await
            {
                tracing::error!(
                    job_id = %claimed.job_id.0,
                    error = %error.message,
                    "unified worker failed to mark extract job completed"
                );
            }
        }
        Err(message) => {
            fail_unified_claimed(
                pool,
                store,
                claimed,
                ApiError::new(
                    "job_runner.extract_failed",
                    ErrorStage::ParsingContent,
                    message,
                ),
            )
            .await;
        }
    }
}

struct ExtractRequest {
    urls: Vec<String>,
    config_json: String,
}

fn extract_request_from_json(
    value: Option<&serde_json::Value>,
) -> Result<ExtractRequest, ApiError> {
    let value = value.ok_or_else(|| {
        ApiError::new(
            "job_runner.missing_request",
            ErrorStage::Planning,
            "extract job has no request payload",
        )
    })?;
    let urls: Vec<String> = value
        .get("urls")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or_else(|| {
            ApiError::new(
                "job_runner.invalid_request",
                ErrorStage::Planning,
                "extract job request is missing a `urls` array",
            )
        })?;
    let config_json = value
        .get("config_json")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    Ok(ExtractRequest { urls, config_json })
}
