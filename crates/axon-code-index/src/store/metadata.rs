use super::CodeIndexStore;
use crate::config::CodeIndexIdentity;
use sqlx::Row;

// Project-level root/status metadata substrate (bead o9y1.1). The read/write
// store APIs are intentionally landed ahead of their consumers (root-hash
// freshness + status reporting in o9y1.2+), so they read as unused until then.
#[allow(dead_code)]
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(serde::Serialize))]
pub(crate) struct ProjectMetadata {
    pub root_hash: Option<String>,
    pub manifest_file_count: u32,
    pub indexed_file_count: u32,
    pub last_indexed_at_ms: i64,
    pub last_refresh_started_at_ms: i64,
    pub last_refresh_finished_at_ms: i64,
    pub last_refresh_status: Option<String>,
    pub last_error_message: Option<String>,
    pub cleanup_debt_count: u32,
}

impl CodeIndexStore {
    #[allow(dead_code)] // substrate for o9y1.2+ (see ProjectMetadata)
    pub(crate) async fn read_project_metadata(
        &self,
        identity: &CodeIndexIdentity,
    ) -> anyhow::Result<Option<ProjectMetadata>> {
        let row = sqlx::query(
            "SELECT root_hash, manifest_file_count, indexed_file_count, \
             last_indexed_at_ms, last_refresh_started_at_ms, \
             last_refresh_finished_at_ms, last_refresh_status, \
             last_error_message, cleanup_debt_count \
             FROM axon_code_projects WHERE project_key = ?",
        )
        .bind(&identity.project_key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| ProjectMetadata {
            root_hash: row.get::<Option<String>, _>("root_hash"),
            manifest_file_count: row.get::<i64, _>("manifest_file_count").max(0) as u32,
            indexed_file_count: row.get::<i64, _>("indexed_file_count").max(0) as u32,
            last_indexed_at_ms: row.get::<i64, _>("last_indexed_at_ms"),
            last_refresh_started_at_ms: row.get::<i64, _>("last_refresh_started_at_ms"),
            last_refresh_finished_at_ms: row.get::<i64, _>("last_refresh_finished_at_ms"),
            last_refresh_status: row.get::<Option<String>, _>("last_refresh_status"),
            last_error_message: row.get::<Option<String>, _>("last_error_message"),
            cleanup_debt_count: row.get::<i64, _>("cleanup_debt_count").max(0) as u32,
        }))
    }

    #[allow(dead_code)] // substrate for o9y1.2+ (see ProjectMetadata)
    pub(crate) async fn write_project_metadata(
        &self,
        identity: &CodeIndexIdentity,
        meta: &ProjectMetadata,
    ) -> anyhow::Result<()> {
        self.upsert_project(identity).await?;
        sqlx::query(
            "UPDATE axon_code_projects SET \
             root_hash = ?, manifest_file_count = ?, indexed_file_count = ?, \
             last_indexed_at_ms = ?, last_refresh_started_at_ms = ?, \
             last_refresh_finished_at_ms = ?, last_refresh_status = ?, \
             last_error_message = ?, cleanup_debt_count = ? \
             WHERE project_key = ?",
        )
        .bind(&meta.root_hash)
        .bind(meta.manifest_file_count as i64)
        .bind(meta.indexed_file_count as i64)
        .bind(meta.last_indexed_at_ms)
        .bind(meta.last_refresh_started_at_ms)
        .bind(meta.last_refresh_finished_at_ms)
        .bind(&meta.last_refresh_status)
        .bind(&meta.last_error_message)
        .bind(meta.cleanup_debt_count as i64)
        .bind(&identity.project_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
