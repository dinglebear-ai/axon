use axon_api::source::*;
use axon_core::sqlite::ImmediateTx;
use axon_error::ErrorStage;
use sqlx::Row;
use uuid::Uuid;

use super::SqliteUnifiedJobStore;
use crate::boundary::Result;
use crate::state_machine::validate_stage_plan;
use crate::unified_codec::{
    enum_name, new_job_descriptor, now_timestamp, optional_to_json, parse_enum, parse_uuid,
    sql_error, to_json,
};

const FINGERPRINT_KEY: &str = "projection_fingerprint_v1";

impl SqliteUnifiedJobStore {
    pub(crate) async fn admit_projection_batch(
        &self,
        admission: ProjectionBatchAdmission,
    ) -> Result<ProjectionBatchAdmissionResult> {
        if admission.items.is_empty() {
            return Err(admission_error(
                "projection.admission_empty",
                "projection admission requires at least one item",
            ));
        }
        let mut tx = ImmediateTx::begin(&self.pool).await.map_err(sql_error)?;
        let mut results = Vec::with_capacity(admission.items.len());
        for (index, item) in admission.items.into_iter().enumerate() {
            validate_stage_plan(&item.request.stage_plan)?;
            let (descriptor, reused) = if let Some(row) = sqlx::query(
                "SELECT job_id, kind, status, created_at, updated_at, metadata_json
                 FROM jobs WHERE idempotency_key = ?",
            )
            .bind(&item.storage_key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(sql_error)?
            {
                let metadata: MetadataMap = serde_json::from_str(row.get("metadata_json"))
                    .map_err(|error| {
                        admission_error("projection.metadata_invalid", error.to_string())
                    })?;
                let stored = metadata
                    .get(FINGERPRINT_KEY)
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                if !constant_time_eq(stored.as_bytes(), item.fingerprint.0.as_bytes()) {
                    return Err(admission_error(
                        "projection.idempotency_collision",
                        "idempotency key was already used for a different request",
                    ));
                }
                (descriptor_from_row(&row)?, true)
            } else {
                (
                    insert_projection_job(&mut tx, item.request.clone(), &item).await?,
                    false,
                )
            };
            sqlx::query(
                "INSERT INTO projection_batch_items
                 (batch_id, item_index, job_id, operation, reused, principal_id, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(admission.batch_id.0.to_string())
            .bind(index as i64)
            .bind(descriptor.job_id.0.to_string())
            .bind(enum_name(item.operation)?)
            .bind(i64::from(reused))
            .bind(&admission.principal_id)
            .bind(now_timestamp().0)
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
            results.push(ProjectionAdmissionResultItem {
                index,
                operation: item.operation,
                descriptor,
                reused,
            });
        }
        tx.commit().await.map_err(sql_error)?;
        Ok(ProjectionBatchAdmissionResult {
            batch_id: admission.batch_id,
            items: results,
        })
    }

    pub(crate) async fn lookup_projection_batch(
        &self,
        lookup: ProjectionBatchLookup,
    ) -> Result<Option<ProjectionBatchAdmissionResult>> {
        let rows = sqlx::query(
            "SELECT p.item_index, p.operation, p.reused,
                    j.job_id, j.kind, j.status, j.created_at, j.updated_at
             FROM projection_batch_items p
             JOIN jobs j ON j.job_id = p.job_id
             WHERE p.principal_id = ? AND p.batch_id = ?
             ORDER BY p.item_index ASC",
        )
        .bind(&lookup.principal_id)
        .bind(lookup.batch_id.0.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        if rows.is_empty() {
            return Ok(None);
        }
        let items = rows
            .into_iter()
            .map(|row| {
                Ok(ProjectionAdmissionResultItem {
                    index: row.get::<i64, _>("item_index") as usize,
                    operation: parse_enum(row.get::<String, _>("operation"))?,
                    descriptor: descriptor_from_row(&row)?,
                    reused: row.get::<i64, _>("reused") != 0,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(ProjectionBatchAdmissionResult {
            batch_id: lookup.batch_id,
            items,
        }))
    }
}

async fn insert_projection_job(
    tx: &mut sqlx::SqliteConnection,
    mut request: JobCreateRequest,
    item: &ProjectionAdmissionItem,
) -> Result<JobDescriptor> {
    let job_id = JobId::new(Uuid::new_v4());
    let now = now_timestamp();
    request.idempotency_key = Some(item.storage_key.clone());
    request.metadata.insert(
        FINGERPRINT_KEY.to_string(),
        serde_json::json!(item.fingerprint.0),
    );
    sqlx::query(
        "INSERT INTO jobs (
            job_id, kind, intent, status, phase, priority, source_id, watch_id,
            parent_job_id, root_job_id, attempt, warnings_json, request_json,
            metadata_json, idempotency_key, auth_snapshot_json, config_snapshot_id,
            stage_plan_json, requirements_json, result_schema, error_json,
            created_at, updated_at, deadline_at
        ) VALUES (?, ?, ?, 'queued', 'queued', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(job_id.0.to_string())
    .bind(enum_name(request.job_kind)?)
    .bind(enum_name(request.job_intent)?)
    .bind(enum_name(request.priority)?)
    .bind(request.source_id.as_ref().map(|id| id.0.as_str()))
    .bind(request.watch_id.as_ref().map(|id| id.0.as_str()))
    .bind(request.parent_job_id.map(|id| id.0.to_string()))
    .bind(request.root_job_id.unwrap_or(job_id).0.to_string())
    .bind(i64::from(request.attempt))
    .bind(to_json(&request.warnings)?)
    .bind(optional_to_json(&request.request)?)
    .bind(to_json(&request.metadata)?)
    .bind(request.idempotency_key.as_deref())
    .bind(to_json(&request.auth_snapshot)?)
    .bind(request.config_snapshot_id.as_ref().map_or("", |id| id.0.as_str()))
    .bind(to_json(&request.stage_plan)?)
    .bind(to_json(&request.requirements)?)
    .bind(request.result_schema.as_deref().unwrap_or(""))
    .bind(optional_to_json(&request.error)?)
    .bind(&now.0)
    .bind(&now.0)
    .bind(request.deadline_at.as_ref().map(|value| value.0.as_str()))
    .execute(&mut *tx)
    .await
    .map_err(sql_error)?;
    for (ordinal, stage) in request.stage_plan.into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO job_stages
             (stage_id, job_id, phase, status, required, provider_requirements_json)
             VALUES (?, ?, ?, 'queued', ?, ?)",
        )
        .bind(stage.stable_id(job_id, ordinal).0.to_string())
        .bind(job_id.0.to_string())
        .bind(enum_name(stage.phase)?)
        .bind(i64::from(stage.required))
        .bind(to_json(&stage.provider_requirements)?)
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
    }
    Ok(new_job_descriptor(job_id, request.job_kind, now))
}

fn descriptor_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<JobDescriptor> {
    let job_id = JobId::new(parse_uuid(row.get::<String, _>("job_id"))?);
    let kind = parse_enum(row.get::<String, _>("kind"))?;
    let created = Timestamp(row.get("created_at"));
    let mut descriptor = new_job_descriptor(job_id, kind, created);
    descriptor.status = parse_enum(row.get::<String, _>("status"))?;
    descriptor.updated_at = Some(Timestamp(row.get("updated_at")));
    Ok(descriptor)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= usize::from(
            left.get(index).copied().unwrap_or_default()
                ^ right.get(index).copied().unwrap_or_default(),
        );
    }
    difference == 0
}

fn admission_error(code: &str, message: impl Into<String>) -> ApiError {
    ApiError::new(code, ErrorStage::Storage, message)
}

#[cfg(test)]
#[path = "projection_admission_tests.rs"]
mod tests;
