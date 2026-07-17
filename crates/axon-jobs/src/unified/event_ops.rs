use axon_api::source::*;
use axon_core::redact::{
    DefaultRedactor, RedactionContext, redact_metadata, redact_text_checked,
    stamp_redaction_metadata,
};

use super::SqliteUnifiedJobStore;
use crate::boundary::Result;
use crate::unified_codec::{ensure_job, enum_name, event_details, sql_error, to_json};

impl SqliteUnifiedJobStore {
    pub(crate) async fn append_job_event(&self, mut event: SourceProgressEvent) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        ensure_job(&mut tx, event.job_id).await?;
        if let Some(dedupe_key) = event.dedupe_key.as_deref() {
            let existing = sqlx::query_scalar::<_, i64>(
                "SELECT sequence FROM job_events WHERE job_id = ? AND dedupe_key = ?",
            )
            .bind(event.job_id.0.to_string())
            .bind(dedupe_key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(sql_error)?;
            if existing.is_some() {
                tx.commit().await.map_err(sql_error)?;
                return Ok(());
            }
        }
        let auto_sequence = event.sequence == 0;
        let sequence = if auto_sequence {
            next_sequence(&mut tx, event.job_id).await?
        } else {
            match validate_explicit_sequence(&mut tx, &event).await? {
                Some(sequence) => sequence,
                None => {
                    tx.commit().await.map_err(sql_error)?;
                    return Ok(());
                }
            }
        };
        event.sequence = sequence;

        let redactor = DefaultRedactor::new();
        let redaction_context = RedactionContext::job_event();
        let redacted_message = redact_text_checked(&redactor, &event.message, &redaction_context)
            .map_err(|status| {
            ApiError::new(
                "job_event.redaction_failed",
                ErrorStage::Publishing,
                format!(
                    "redaction failed with status `{}` for job event message",
                    status.as_str()
                ),
            )
        })?;
        event.validate_bounds().map_err(|error| *error)?;
        let (details, redaction_report) =
            redact_metadata(event_details(&event), &redaction_context, &redactor);
        let details = stamp_redaction_metadata(details, &redaction_report);
        sqlx::query(
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
        .bind(event.dedupe_key.as_deref())
        .bind(to_json(&details)?)
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
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
}

async fn next_sequence(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, job_id: JobId) -> Result<u64> {
    let sequence = sqlx::query_scalar::<_, i64>(
        "UPDATE jobs
         SET last_event_sequence = last_event_sequence + 1
         WHERE job_id = ?
         RETURNING last_event_sequence",
    )
    .bind(job_id.0.to_string())
    .fetch_one(&mut **tx)
    .await
    .map_err(sql_error)?;
    Ok(sequence as u64)
}

async fn validate_explicit_sequence(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: &SourceProgressEvent,
) -> Result<Option<u64>> {
    let last_sequence =
        sqlx::query_scalar::<_, i64>("SELECT last_event_sequence FROM jobs WHERE job_id = ?")
            .bind(event.job_id.0.to_string())
            .fetch_one(&mut **tx)
            .await
            .map_err(sql_error)? as u64;
    let expected = last_sequence + 1;
    if event.sequence == expected {
        return Ok(Some(event.sequence));
    }
    if let Some(dedupe_key) = event.dedupe_key.as_deref() {
        let duplicate_sequence = sqlx::query_scalar::<_, i64>(
            "SELECT sequence FROM job_events WHERE job_id = ? AND dedupe_key = ?",
        )
        .bind(event.job_id.0.to_string())
        .bind(dedupe_key)
        .fetch_optional(&mut **tx)
        .await
        .map_err(sql_error)?;
        if duplicate_sequence == Some(event.sequence as i64) {
            return Ok(None);
        }
    }
    Err(ApiError::new(
        "job_event.sequence_invalid",
        ErrorStage::Publishing,
        format!(
            "expected event sequence {} for job {}, got {}",
            expected, event.job_id.0, event.sequence
        ),
    ))
}
