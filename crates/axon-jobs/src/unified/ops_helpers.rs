use axon_api::source::{ApiError, ErrorStage, JobEventListRequest, JobListRequest};
use sqlx::query::Query;
use sqlx::sqlite::SqliteArguments;
use sqlx::{Row, Sqlite};

use crate::boundary::Result;
use crate::unified_codec::{enum_name, sql_error};

pub(super) fn optional_u64(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<Option<u64>> {
    row.try_get::<Option<i64>, _>(column)
        .map_err(sql_error)?
        .map(|value| {
            u64::try_from(value).map_err(|_| {
                ApiError::new(
                    "job.counts_invalid",
                    ErrorStage::Retrieving,
                    format!("negative historical counter in {column}"),
                )
            })
        })
        .transpose()
}

pub(super) struct JobFilterBindings {
    pub(super) source_id: Option<String>,
    pub(super) watch_id: Option<String>,
}

pub(super) fn append_job_filters(
    sql: &mut String,
    request: &JobListRequest,
) -> Result<JobFilterBindings> {
    if let Some(status) = request.status {
        sql.push_str(&format!(" AND status = '{}'", enum_name(status)?));
    }
    if let Some(kind) = request.kind {
        sql.push_str(&format!(" AND kind = '{}'", enum_name(kind)?));
    }
    if let Some(source_id) = &request.source_id {
        sql.push_str(" AND source_id = ?");
        let source_id = Some(source_id.0.clone());
        let watch_id = request.watch_id.as_ref().map(|watch_id| watch_id.0.clone());
        if watch_id.is_some() {
            sql.push_str(" AND watch_id = ?");
        }
        return Ok(JobFilterBindings {
            source_id,
            watch_id,
        });
    }
    if let Some(watch_id) = &request.watch_id {
        sql.push_str(" AND watch_id = ?");
        return Ok(JobFilterBindings {
            source_id: None,
            watch_id: Some(watch_id.0.clone()),
        });
    }
    Ok(JobFilterBindings {
        source_id: None,
        watch_id: None,
    })
}

pub(super) fn bind_job_filters<'q>(
    mut query: Query<'q, Sqlite, SqliteArguments<'q>>,
    bindings: &'q JobFilterBindings,
) -> Query<'q, Sqlite, SqliteArguments<'q>> {
    if let Some(source_id) = bindings.source_id.as_deref() {
        query = query.bind(source_id);
    }
    if let Some(watch_id) = bindings.watch_id.as_deref() {
        query = query.bind(watch_id);
    }
    query
}

pub(super) fn append_event_filters(sql: &mut String, request: &JobEventListRequest) -> Result<()> {
    if let Some(phase) = request.phase {
        sql.push_str(&format!(" AND phase = '{}'", enum_name(phase)?));
    }
    if let Some(severity) = request.severity {
        sql.push_str(&format!(" AND severity = '{}'", enum_name(severity)?));
    }
    if let Some(visibility) = request.visibility {
        sql.push_str(&format!(" AND visibility = '{}'", enum_name(visibility)?));
    } else {
        sql.push_str(" AND visibility IN ('public', 'redacted', 'derived')");
    }
    if let Some(since_sequence) = request.since_sequence {
        sql.push_str(&format!(" AND sequence > {since_sequence}"));
    }
    Ok(())
}
