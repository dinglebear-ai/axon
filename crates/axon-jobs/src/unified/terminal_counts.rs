use axon_api::source::{JobId, StageCounts};

use super::SqliteUnifiedJobStore;
use super::ops_helpers::optional_u64;
use crate::boundary::Result;
use crate::unified_codec::sql_error;

impl SqliteUnifiedJobStore {
    pub(crate) async fn terminal_counts_from_events(
        &self,
        job_id: JobId,
    ) -> Result<Option<StageCounts>> {
        let row = sqlx::query(
            "WITH latest_attempt AS (
                SELECT MAX(attempt) AS attempt FROM job_events WHERE job_id = ?
             )
             SELECT
                MAX(json_extract(details_json, '$.source_progress_event.counts.items_total')) AS items_total,
                MAX(json_extract(details_json, '$.source_progress_event.counts.items_done')) AS items_done,
                MAX(json_extract(details_json, '$.source_progress_event.counts.documents_total')) AS documents_total,
                MAX(json_extract(details_json, '$.source_progress_event.counts.documents_done')) AS documents_done,
                MAX(json_extract(details_json, '$.source_progress_event.counts.chunks_total')) AS chunks_total,
                MAX(json_extract(details_json, '$.source_progress_event.counts.chunks_done')) AS chunks_done,
                MAX(json_extract(details_json, '$.source_progress_event.counts.bytes_total')) AS bytes_total,
                MAX(json_extract(details_json, '$.source_progress_event.counts.bytes_done')) AS bytes_done
             FROM job_events, latest_attempt
             WHERE job_id = ? AND job_events.attempt = latest_attempt.attempt
               AND json_type(details_json, '$.source_progress_event.counts') = 'object'",
        )
        .bind(job_id.0.to_string())
        .bind(job_id.0.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(sql_error)?;

        let fields = [
            optional_u64(&row, "items_total")?,
            optional_u64(&row, "items_done")?,
            optional_u64(&row, "documents_total")?,
            optional_u64(&row, "documents_done")?,
            optional_u64(&row, "chunks_total")?,
            optional_u64(&row, "chunks_done")?,
            optional_u64(&row, "bytes_total")?,
            optional_u64(&row, "bytes_done")?,
        ];
        if fields.iter().all(Option::is_none) {
            return Ok(None);
        }
        Ok(Some(StageCounts {
            items_total: fields[0],
            items_done: fields[1].unwrap_or(0),
            documents_total: fields[2],
            documents_done: fields[3].unwrap_or(0),
            chunks_total: fields[4],
            chunks_done: fields[5].unwrap_or(0),
            bytes_total: fields[6],
            bytes_done: fields[7].unwrap_or(0),
        }))
    }
}
