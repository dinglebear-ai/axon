use sqlx::Row;

use crate::store::CodeIndexStore;

impl CodeIndexStore {
    pub(crate) async fn init_schema(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS axon_code_files (
              project_key TEXT NOT NULL,
              relative_path TEXT NOT NULL,
              hash TEXT NOT NULL,
              size_bytes INTEGER NOT NULL,
              mtime_ns INTEGER NOT NULL,
              indexed_generation INTEGER NOT NULL,
              pending INTEGER NOT NULL DEFAULT 0,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY (project_key, relative_path)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS axon_code_projects (
              project_key TEXT PRIMARY KEY,
              project_display TEXT NOT NULL,
              project_root TEXT NOT NULL,
              collection TEXT NOT NULL,
              embedder_key TEXT NOT NULL,
              index_version INTEGER NOT NULL,
              committed_generation INTEGER NOT NULL DEFAULT 0,
              max_generation INTEGER NOT NULL DEFAULT 0,
              lease_owner TEXT,
              lease_expires_at_ms INTEGER NOT NULL DEFAULT 0,
              last_checked_at_ms INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
        self.ensure_project_column("max_generation", "INTEGER NOT NULL DEFAULT 0")
            .await?;
        self.ensure_project_column("root_hash", "TEXT").await?;
        self.ensure_project_column("manifest_file_count", "INTEGER DEFAULT 0")
            .await?;
        self.ensure_project_column("indexed_file_count", "INTEGER DEFAULT 0")
            .await?;
        self.ensure_project_column("last_indexed_at_ms", "INTEGER DEFAULT 0")
            .await?;
        self.ensure_project_column("last_refresh_started_at_ms", "INTEGER DEFAULT 0")
            .await?;
        self.ensure_project_column("last_refresh_finished_at_ms", "INTEGER DEFAULT 0")
            .await?;
        self.ensure_project_column("last_refresh_status", "TEXT")
            .await?;
        self.ensure_project_column("last_error_message", "TEXT")
            .await?;
        self.ensure_project_column("cleanup_debt_count", "INTEGER DEFAULT 0")
            .await?;

        Ok(())
    }

    async fn ensure_project_column(
        &self,
        column_name: &str,
        column_definition: &str,
    ) -> anyhow::Result<()> {
        let rows = sqlx::query("PRAGMA table_info(axon_code_projects)")
            .fetch_all(&self.pool)
            .await?;
        let exists = rows
            .iter()
            .any(|row| row.get::<String, _>("name") == column_name);
        if !exists {
            let query = format!(
                "ALTER TABLE axon_code_projects ADD COLUMN {column_name} {column_definition}"
            );
            sqlx::query(&query).execute(&self.pool).await?;
        }
        Ok(())
    }
}
