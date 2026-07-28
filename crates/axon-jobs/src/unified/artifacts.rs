use axon_api::source::{ApiError, ArtifactRef, ErrorStage, JobId};

use super::SqliteUnifiedJobStore;
use crate::boundary::Result;
use crate::unified_codec::{ensure_job_pool, enum_name, sql_error};

impl SqliteUnifiedJobStore {
    /// Persist artifacts emitted by a completed job so transport callers can
    /// discover them through the unified job lifecycle.
    pub async fn record_job_artifacts(
        &self,
        job_id: JobId,
        artifacts: &[ArtifactRef],
    ) -> Result<()> {
        ensure_job_pool(&self.pool, job_id).await?;
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        for artifact in artifacts {
            let size_bytes = artifact
                .size_bytes
                .map(i64::try_from)
                .transpose()
                .map_err(|_| {
                    ApiError::new(
                        "job_artifact.invalid_size",
                        ErrorStage::Publishing,
                        format!(
                            "artifact {} exceeds SQLite integer range",
                            artifact.artifact_id.0
                        ),
                    )
                })?;
            sqlx::query(
                "INSERT INTO job_artifacts (artifact_id, job_id, artifact_kind, uri, size_bytes, content_hash, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(artifact_id) DO UPDATE SET
                   job_id = excluded.job_id,
                   artifact_kind = excluded.artifact_kind,
                   uri = excluded.uri,
                   size_bytes = excluded.size_bytes,
                   content_hash = excluded.content_hash,
                   created_at = excluded.created_at",
            )
            .bind(artifact.artifact_id.0.to_string())
            .bind(job_id.0.to_string())
            .bind(enum_name(artifact.artifact_kind)?)
            .bind(&artifact.uri)
            .bind(size_bytes)
            .bind(&artifact.content_hash)
            .bind(&artifact.created_at.0)
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
        }
        tx.commit().await.map_err(sql_error)?;
        Ok(())
    }
}
