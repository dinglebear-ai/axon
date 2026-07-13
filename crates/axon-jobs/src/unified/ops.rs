use axon_api::source::*;
use axon_core::redact::{DefaultRedactor, RedactionContext, Redactor, redact_metadata};
use sqlx::Row;
use sqlx::Sqlite;
use sqlx::query::Query;
use sqlx::sqlite::SqliteArguments;
use uuid::Uuid;

use super::SqliteUnifiedJobStore;
use crate::boundary::Result;
use crate::limits::clamp_page_limit;
use crate::state_machine::validate_transition;
use crate::unified::pagination::{
    EventCursor, JobCursor, decode_event_cursor, decode_job_cursor, encode_event_cursor,
    encode_job_cursor,
};
use crate::unified_codec::*;

impl SqliteUnifiedJobStore {
    pub(crate) async fn create_job(&self, request: JobCreateRequest) -> Result<JobDescriptor> {
        if let Some(idempotency_key) = request.idempotency_key.as_deref()
            && let Some(summary) = find_by_idempotency_key(&self.pool, idempotency_key).await?
        {
            return Ok(descriptor(&summary));
        }

        let job_id = JobId::new(Uuid::new_v4());
        let root_job_id = request.root_job_id.unwrap_or(job_id);
        let now = now_timestamp();
        let request_json = request.request.clone();
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        sqlx::query(
            "INSERT INTO jobs (
                job_id, kind, intent, status, phase, priority, source_id, watch_id,
                parent_job_id, root_job_id, attempt, warnings_json, request_json,
                metadata_json, idempotency_key, auth_snapshot_json, config_snapshot_id,
                stage_plan_json, requirements_json, result_schema, error_json,
                created_at, updated_at, deadline_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(job_id.0.to_string())
        .bind(enum_name(request.job_kind)?)
        .bind(enum_name(request.job_intent)?)
        .bind(enum_name(LifecycleStatus::Queued)?)
        .bind(enum_name(PipelinePhase::Queued)?)
        .bind(enum_name(request.priority)?)
        .bind(request.source_id.as_ref().map(|id| id.0.as_str()))
        .bind(request.watch_id.as_ref().map(|id| id.0.as_str()))
        .bind(request.parent_job_id.map(|id| id.0.to_string()))
        .bind(root_job_id.0.to_string())
        .bind(request.attempt as i64)
        .bind(to_json(&request.warnings)?)
        .bind(optional_to_json(&request_json)?)
        .bind(to_json(&request.metadata)?)
        .bind(request.idempotency_key.as_deref())
        .bind(to_json(&request.auth_snapshot)?)
        .bind(
            request
                .config_snapshot_id
                .as_ref()
                .map(|id| id.0.as_str())
                .unwrap_or(""),
        )
        .bind(to_json(&request.stage_plan)?)
        .bind(to_json(&request.requirements)?)
        .bind(request.result_schema.as_deref().unwrap_or(""))
        .bind(optional_to_json(&request.error)?)
        .bind(now.0.as_str())
        .bind(now.0.as_str())
        .bind(request.deadline_at.as_ref().map(|ts| ts.0.as_str()))
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;

        for stage in request.stage_plan {
            sqlx::query(
                "INSERT INTO job_stages (
                    stage_id, job_id, phase, status, required, provider_requirements_json
                ) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(job_id.0.to_string())
            .bind(enum_name(stage.phase)?)
            .bind(enum_name(LifecycleStatus::Queued)?)
            .bind(if stage.required { 1_i64 } else { 0_i64 })
            .bind(to_json(&stage.provider_requirements)?)
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
        }

        tx.commit().await.map_err(sql_error)?;
        Ok(new_job_descriptor(job_id, request.job_kind, now))
    }

    pub(crate) async fn get_job(&self, job_id: JobId) -> Result<Option<JobSummary>> {
        let row = sqlx::query("SELECT * FROM jobs WHERE job_id = ?")
            .bind(job_id.0.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.map(row_to_summary).transpose()
    }

    pub(crate) async fn job_attempts(&self, job_id: JobId) -> Result<Vec<JobAttemptSnapshot>> {
        ensure_job_pool(&self.pool, job_id).await?;
        let rows = sqlx::query("SELECT * FROM job_attempts WHERE job_id = ? ORDER BY attempt ASC")
            .bind(job_id.0.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(sql_error)?;
        rows.into_iter().map(row_to_attempt).collect()
    }

    pub(crate) async fn job_stages(&self, job_id: JobId) -> Result<Vec<JobStageSnapshot>> {
        ensure_job_pool(&self.pool, job_id).await?;
        let rows = sqlx::query("SELECT * FROM job_stages WHERE job_id = ? ORDER BY rowid ASC")
            .bind(job_id.0.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(sql_error)?;
        rows.into_iter().map(row_to_stage).collect()
    }

    pub(crate) async fn update_job_status(&self, status: JobStatusUpdate) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        let row = sqlx::query("SELECT status, started_at FROM jobs WHERE job_id = ?")
            .bind(status.job_id.0.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(sql_error)?
            .ok_or_else(|| missing_job(status.job_id))?;
        let current = parse_enum::<LifecycleStatus>(row.get::<String, _>("status"))?;
        validate_transition(status.job_id, current, status.status)?;
        let now = now_timestamp();
        let job_started_at = (row.get::<Option<String>, _>("started_at").is_none()
            && status.status == LifecycleStatus::Running)
            .then(|| now.0.clone());
        let stage_started_at = (status.status == LifecycleStatus::Running).then(|| now.0.clone());
        let finished_at = is_terminal(status.status).then(|| now.0.clone());

        // cooldown_until: cleared on every transition to a non-Waiting status
        // (a job that cooled once and later runs/completes/fails must not
        // retain a stale cooldown that silently blocks its next legitimate
        // claim). Left untouched when the new status IS Waiting so a
        // heartbeat/update that re-affirms Waiting does not wipe out a
        // cooldown set separately via `apply_provider_cooling`.
        let status_name = enum_name(status.status)?;
        sqlx::query(
            "UPDATE jobs SET
                source_id = COALESCE(?, source_id),
                status = ?, phase = ?, counts_json = ?, current_json = ?,
                last_error_json = ?, updated_at = ?,
                started_at = COALESCE(started_at, ?),
                finished_at = COALESCE(?, finished_at),
                cooldown_until = CASE WHEN ? = 'waiting' THEN cooldown_until ELSE NULL END
             WHERE job_id = ?",
        )
        .bind(
            status
                .source_id
                .as_ref()
                .map(|source_id| source_id.0.as_str()),
        )
        .bind(status_name.as_str())
        .bind(enum_name(status.phase)?)
        .bind(optional_to_json(&status.counts)?)
        .bind(optional_to_json(&status.current)?)
        .bind(optional_to_json(&status.error)?)
        .bind(now.0.as_str())
        .bind(job_started_at.as_deref())
        .bind(finished_at.as_deref())
        .bind(status_name.as_str())
        .bind(status.job_id.0.to_string())
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;

        if let Some(stage_id) = status.stage_id {
            self.update_stage_status(&mut tx, &status, stage_id, &stage_started_at, &finished_at)
                .await?;
        }
        tx.commit().await.map_err(sql_error)?;

        // Supplement: record this transition durably in the observability sink
        // (strictly-increasing per-job sequence + heartbeat). Runs after the
        // authoritative status write commits; sink errors are logged, not
        // propagated, so the observe stream never fails the status update.
        self.observe_status(&status).await;
        Ok(())
    }

    async fn update_stage_status(
        &self,
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        status: &JobStatusUpdate,
        stage_id: StageId,
        started_at: &Option<String>,
        finished_at: &Option<String>,
    ) -> Result<()> {
        let result = sqlx::query(
            "UPDATE job_stages SET
                status = ?,
                counts_json = ?,
                started_at = COALESCE(started_at, ?),
                completed_at = COALESCE(?, completed_at),
                error_json = ?
             WHERE stage_id = ? AND job_id = ?",
        )
        .bind(enum_name(status.status)?)
        .bind(optional_to_json(&status.counts)?)
        .bind(started_at.as_deref())
        .bind(finished_at.as_deref())
        .bind(optional_to_json(&status.error)?)
        .bind(stage_id.0.to_string())
        .bind(status.job_id.0.to_string())
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
        if result.rows_affected() == 0 {
            return Err(ApiError::new(
                "job_stage.not_found",
                ErrorStage::Publishing,
                format!("stage {} not found for job {}", stage_id.0, status.job_id.0),
            ));
        }
        Ok(())
    }

    pub(crate) async fn append_job_event(&self, mut event: SourceProgressEvent) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        ensure_job(&mut tx, event.job_id).await?;
        let auto_sequence = event.sequence == 0;
        let sequence = if auto_sequence {
            sqlx::query_scalar::<_, i64>(
                "UPDATE jobs
                 SET last_event_sequence = last_event_sequence + 1
                 WHERE job_id = ?
                 RETURNING last_event_sequence",
            )
            .bind(event.job_id.0.to_string())
            .fetch_one(&mut *tx)
            .await
            .map_err(sql_error)? as u64
        } else {
            let last_sequence = sqlx::query_scalar::<_, i64>(
                "SELECT last_event_sequence FROM jobs WHERE job_id = ?",
            )
            .bind(event.job_id.0.to_string())
            .fetch_one(&mut *tx)
            .await
            .map_err(sql_error)? as u64;
            let expected = last_sequence + 1;
            if event.sequence != expected {
                if let Some(dedupe_key) = event.dedupe_key.as_deref() {
                    let duplicate_sequence = sqlx::query_scalar::<_, i64>(
                        "SELECT sequence FROM job_events WHERE job_id = ? AND dedupe_key = ?",
                    )
                    .bind(event.job_id.0.to_string())
                    .bind(dedupe_key)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(sql_error)?;
                    if duplicate_sequence == Some(event.sequence as i64) {
                        tx.commit().await.map_err(sql_error)?;
                        return Ok(());
                    }
                }
                return Err(ApiError::new(
                    "job_event.sequence_invalid",
                    ErrorStage::Publishing,
                    format!(
                        "expected event sequence {} for job {}, got {}",
                        expected, event.job_id.0, event.sequence
                    ),
                ));
            }
            event.sequence
        };
        event.sequence = sequence;
        let duplicate_dedupe = if let Some(dedupe_key) = event.dedupe_key.as_deref() {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM job_events WHERE job_id = ? AND dedupe_key = ?",
            )
            .bind(event.job_id.0.to_string())
            .bind(dedupe_key)
            .fetch_one(&mut *tx)
            .await
            .map_err(sql_error)?
                > 0
        } else {
            false
        };
        // Fail-closed redaction boundary: `message` (its own column) and
        // `details_json` (the full serialized event, including any
        // `error.message`/adapter-supplied fields) both cross into a row
        // that is later returned verbatim over REST/MCP/CLI. Neither is
        // trusted content — scrub before the write, not after.
        let redactor = DefaultRedactor::new();
        let redaction_context = RedactionContext::job_event();
        let redacted_message = redactor.redact_text(&event.message, &redaction_context);
        let (mut details, _redaction_report) =
            redact_metadata(event_details(&event), &redaction_context, &redactor);
        let dedupe_key = if duplicate_dedupe {
            details.insert("dedupe_duplicate".to_string(), serde_json::json!(true));
            None
        } else {
            event.dedupe_key.as_deref()
        };
        let result = sqlx::query(
            "INSERT INTO job_events (
                event_id, job_id, sequence, attempt, stage_id, phase, status, severity,
                visibility, message, timestamp, dedupe_key, details_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(event.event_id)
        .bind(event.job_id.0.to_string())
        .bind(event.sequence as i64)
        .bind(event.attempt as i64)
        .bind(event.stage_id.map(|id| id.0.to_string()))
        .bind(enum_name(event.phase)?)
        .bind(enum_name(event.status)?)
        .bind(enum_name(event.severity)?)
        .bind(enum_name(event.visibility)?)
        .bind(redacted_message)
        .bind(event.timestamp.0)
        .bind(dedupe_key)
        .bind(to_json(&details)?)
        .execute(&mut *tx)
        .await;
        if let Err(error) = result {
            return Err(sql_error(error));
        }
        if !auto_sequence {
            sqlx::query("UPDATE jobs SET last_event_sequence = ? WHERE job_id = ?")
                .bind(event.sequence as i64)
                .bind(event.job_id.0.to_string())
                .execute(&mut *tx)
                .await
                .map_err(sql_error)?;
        }
        tx.commit().await.map_err(sql_error)
    }

    pub(crate) async fn list_jobs(&self, request: JobListRequest) -> Result<Page<JobSummary>> {
        let mut sql = "SELECT * FROM jobs WHERE 1 = 1".to_string();
        let bindings = append_job_filters(&mut sql, &request)?;
        let cursor = request
            .cursor
            .as_deref()
            .map(decode_job_cursor)
            .transpose()
            .map_err(|message| {
                ApiError::new("job.cursor_invalid", ErrorStage::Retrieving, message)
            })?;
        if cursor.is_some() {
            sql.push_str(" AND (updated_at < ? OR (updated_at = ? AND job_id < ?))");
        }
        let total = if cursor.is_none() {
            let total_sql = sql.replacen("SELECT *", "SELECT COUNT(*)", 1);
            let mut total_query = sqlx::query_scalar::<_, i64>(&total_sql);
            if let Some(source_id) = bindings.source_id.as_deref() {
                total_query = total_query.bind(source_id);
            }
            if let Some(watch_id) = bindings.watch_id.as_deref() {
                total_query = total_query.bind(watch_id);
            }
            Some(total_query.fetch_one(&self.pool).await.map_err(sql_error)? as u64)
        } else {
            None
        };
        let limit = clamp_page_limit(request.limit);
        sql.push_str(" ORDER BY updated_at DESC, job_id DESC LIMIT ");
        sql.push_str(&limit.to_string());
        let mut query = bind_job_filters(sqlx::query(&sql), &bindings);
        if let Some(cursor) = cursor.as_ref() {
            query = query
                .bind(cursor.updated_at.as_str())
                .bind(cursor.updated_at.as_str())
                .bind(cursor.job_id.as_str());
        }
        let rows = query.fetch_all(&self.pool).await.map_err(sql_error)?;
        let items = rows
            .into_iter()
            .map(row_to_summary)
            .collect::<Result<Vec<_>>>()?;
        Ok(Page {
            limit,
            total,
            next_cursor: items
                .last()
                .filter(|_| items.len() == limit as usize)
                .map(|job| {
                    encode_job_cursor(&JobCursor {
                        updated_at: job.updated_at.0.clone(),
                        job_id: job.job_id.0.to_string(),
                    })
                }),
            items,
        })
    }

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
        sql.push_str(&limit.to_string());
        let rows = sqlx::query(&sql)
            .bind(request.job_id.0.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(sql_error)?;
        let events = rows
            .into_iter()
            .map(row_to_event)
            .collect::<Result<Vec<_>>>()?;
        Ok(JobEventPage {
            last_sequence: events.last().map(|event| event.sequence).unwrap_or(0),
            limit,
            next_cursor: events
                .last()
                .filter(|_| events.len() == limit as usize)
                .map(|event| {
                    encode_event_cursor(&EventCursor {
                        sequence: event.sequence,
                    })
                }),
            events,
        })
    }

    pub(crate) async fn latest_sequence(&self, job_id: JobId) -> Result<Option<u64>> {
        ensure_job_pool(&self.pool, job_id).await?;
        let sequence = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(sequence) FROM job_events WHERE job_id = ?",
        )
        .bind(job_id.0.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(sql_error)?
        .map(|sequence| sequence as u64);
        Ok(sequence)
    }
}

struct JobFilterBindings {
    source_id: Option<String>,
    watch_id: Option<String>,
}

fn append_job_filters(sql: &mut String, request: &JobListRequest) -> Result<JobFilterBindings> {
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

fn bind_job_filters<'q>(
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

fn append_event_filters(sql: &mut String, request: &JobEventListRequest) -> Result<()> {
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
