use axon_api::source::{ApiError, ErrorStage, JobId, SourceWarning};
use sqlx::Row;

use crate::boundary::Result;
use crate::unified_codec::{from_json, sql_error, to_json};

pub(super) async fn collect_terminal_warnings(
    tx: &mut sqlx::SqliteConnection,
    job_id: JobId,
    existing_json: String,
) -> Result<String> {
    let mut warnings = from_json::<Vec<SourceWarning>>(existing_json)?;
    let rows =
        sqlx::query("SELECT details_json FROM job_events WHERE job_id = ? ORDER BY sequence ASC")
            .bind(job_id.0.to_string())
            .fetch_all(&mut *tx)
            .await
            .map_err(sql_error)?;
    for row in rows {
        let details = from_json::<serde_json::Value>(row.get::<String, _>("details_json"))?;
        let Some(value) = details.pointer("/source_progress_event/warning") else {
            continue;
        };
        let warning = serde_json::from_value::<SourceWarning>(value.clone()).map_err(|error| {
            ApiError::new(
                "job.warning_decode_failed",
                ErrorStage::Publishing,
                format!("decode redacted job warning: {error}"),
            )
        })?;
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
    }
    to_json(&warnings)
}
