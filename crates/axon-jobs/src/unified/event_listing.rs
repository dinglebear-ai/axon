use axon_api::source::{ApiError, ErrorStage, JobEventListRequest, JobEventPage};

use super::SqliteUnifiedJobStore;
use super::ops_helpers::append_event_filters;
use super::pagination::{EventCursor, decode_event_cursor, encode_event_cursor};
use crate::boundary::Result;
use crate::limits::clamp_page_limit;
use crate::unified_codec::{reject_non_public_visibility, row_to_event, sql_error};

impl SqliteUnifiedJobStore {
    pub(crate) async fn list_events(&self, request: JobEventListRequest) -> Result<JobEventPage> {
        reject_non_public_visibility(request.visibility)?;
        let mut sql = "SELECT * FROM job_events WHERE job_id = ?".to_string();
        let cursor_sequence = request
            .cursor
            .as_deref()
            .map(decode_event_cursor)
            .transpose()
            .map_err(|message| {
                ApiError::new("job_event.cursor_invalid", ErrorStage::Retrieving, message)
            })?
            .map(|cursor| cursor.sequence);
        let after_sequence = cursor_sequence.or(request.after_sequence);
        if let Some(after_sequence) = after_sequence {
            sql.push_str(&format!(" AND sequence > {after_sequence}"));
        }
        append_event_filters(&mut sql, &request)?;
        let limit = clamp_page_limit(request.limit);
        sql.push_str(" ORDER BY sequence ASC LIMIT ");
        sql.push_str(&(limit + 1).to_string());
        let rows = sqlx::query(&sql)
            .bind(request.job_id.0.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(sql_error)?;
        let mut events = rows
            .into_iter()
            .map(row_to_event)
            .collect::<Result<Vec<_>>>()?;
        let has_more = events.len() > limit as usize;
        if has_more {
            events.truncate(limit as usize);
        }
        Ok(JobEventPage {
            last_sequence: events.last().map(|event| event.sequence).unwrap_or(0),
            limit,
            next_cursor: events.last().filter(|_| has_more).map(|event| {
                encode_event_cursor(&EventCursor {
                    sequence: event.sequence,
                })
            }),
            events,
        })
    }
}
