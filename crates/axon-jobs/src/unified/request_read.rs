//! Standalone read of the unified `jobs.request_json` column.
//!
//! Split out of `unified/ops.rs` (which is already at the 500-line monolith
//! cap) rather than growing it further. `JobSummary` intentionally does not
//! carry the original `request` payload passed to `JobStore::create()` --
//! see the doc comment on `JobStore::request_json` in `boundary.rs` -- so
//! callers that need it back (e.g. the Extract CLI/MCP/REST bridge in
//! `axon-services`) call this instead of re-fetching the whole row via
//! `get_job`.

use axon_api::source::JobId;
use sqlx::Row;

use super::SqliteUnifiedJobStore;
use crate::boundary::Result;
use crate::unified_codec::{from_optional_json, sql_error};

impl SqliteUnifiedJobStore {
    /// Read back the `request_json` column captured at `create()` time,
    /// without pulling the rest of the row. Returns `Ok(None)` both when the
    /// job doesn't exist and when it exists but stored no request payload --
    /// callers that need to distinguish those cases should call `get_job`
    /// first.
    pub(crate) async fn get_job_request_json(
        &self,
        job_id: JobId,
    ) -> Result<Option<serde_json::Value>> {
        let row = sqlx::query("SELECT request_json FROM jobs WHERE job_id = ?")
            .bind(job_id.0.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let request_json = row.get::<Option<String>, _>("request_json");
        from_optional_json(request_json)
    }
}
