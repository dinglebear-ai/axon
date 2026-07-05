use std::collections::{HashMap, HashSet};

use sqlx::{Row, SqlitePool};

use crate::config::CodeIndexIdentity;
use crate::manifest::{FileDiff, FileManifestEntry, ManifestSnapshot};

mod metadata;
#[cfg(test)]
pub(crate) use metadata::ProjectMetadata;

#[derive(Clone)]
pub struct CodeIndexStore {
    pub(super) pool: SqlitePool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredFile {
    pub hash: String,
    pub size_bytes: u64,
    pub mtime_ns: i64,
    pub indexed_generation: i64,
    pub pending: bool,
}

impl CodeIndexStore {
    pub async fn open_for_pool(pool: SqlitePool) -> anyhow::Result<Self> {
        let store = Self { pool };
        store.init_schema().await?;
        Ok(store)
    }

    #[cfg(test)]
    pub(crate) async fn open_in_memory() -> anyhow::Result<Self> {
        let pool = axon_jobs::store::open_sqlite_pool(":memory:").await?;
        Ok(Self { pool })
    }

    pub(crate) async fn lookup_file(
        &self,
        identity: &CodeIndexIdentity,
        path: &str,
    ) -> anyhow::Result<Option<StoredFile>> {
        let row = sqlx::query(
            r#"
            SELECT hash, size_bytes, mtime_ns, indexed_generation, pending
            FROM axon_code_files
            WHERE project_key = ? AND relative_path = ?
            "#,
        )
        .bind(&identity.project_key)
        .bind(path)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| StoredFile {
            hash: row.get::<String, _>("hash"),
            size_bytes: row.get::<i64, _>("size_bytes") as u64,
            mtime_ns: row.get::<i64, _>("mtime_ns"),
            indexed_generation: row.get::<i64, _>("indexed_generation"),
            pending: row.get::<i64, _>("pending") != 0,
        }))
    }

    pub(crate) async fn diff_manifest(
        &self,
        identity: &CodeIndexIdentity,
        manifest: &ManifestSnapshot,
    ) -> anyhow::Result<FileDiff> {
        let stored = self.files_for_project(identity).await?;
        let committed_generation = self.committed_generation(identity).await?.unwrap_or(0);
        let manifest_paths = manifest
            .files
            .iter()
            .map(|entry| entry.relative_path.as_str())
            .collect::<HashSet<_>>();
        let mut diff = FileDiff::default();

        for entry in &manifest.files {
            match stored.get(&entry.relative_path) {
                None => diff.added.push(entry.clone()),
                Some(file)
                    if file.pending
                        || file.indexed_generation > committed_generation
                        || entry.hash.as_deref() != Some(file.hash.as_str())
                        || entry.size_bytes != file.size_bytes
                        || entry.mtime_ns != file.mtime_ns =>
                {
                    diff.modified.push(entry.clone());
                }
                Some(_) => {}
            }
        }

        for path in stored.keys() {
            if !manifest_paths.contains(path.as_str()) {
                diff.removed.push(path.clone());
            }
        }
        diff.removed.sort();
        Ok(diff)
    }

    pub(crate) async fn completed_uncommitted_generation(
        &self,
        identity: &CodeIndexIdentity,
        manifest: &ManifestSnapshot,
    ) -> anyhow::Result<Option<i64>> {
        let stored = self.files_for_project(identity).await?;
        let committed_generation = self.committed_generation(identity).await?.unwrap_or(0);
        let manifest_paths = manifest
            .files
            .iter()
            .map(|entry| entry.relative_path.as_str())
            .collect::<HashSet<_>>();
        if stored
            .keys()
            .any(|path| !manifest_paths.contains(path.as_str()))
        {
            return Ok(None);
        }

        let mut candidate = None;
        for entry in &manifest.files {
            let Some(file) = stored.get(&entry.relative_path) else {
                return Ok(None);
            };
            if file.pending
                || file.indexed_generation <= committed_generation
                || entry.hash.as_deref() != Some(file.hash.as_str())
                || entry.size_bytes != file.size_bytes
                || entry.mtime_ns != file.mtime_ns
            {
                return Ok(None);
            }
            match candidate {
                Some(generation) if generation != file.indexed_generation => return Ok(None),
                Some(_) => {}
                None => candidate = Some(file.indexed_generation),
            }
        }
        Ok(candidate)
    }

    pub(crate) async fn acquire_lease(
        &self,
        identity: &CodeIndexIdentity,
        owner: &str,
        ttl_ms: i64,
    ) -> anyhow::Result<bool> {
        self.upsert_project(identity).await?;
        let now = now_ms();
        let expires = now + ttl_ms;
        let result = sqlx::query(
            r#"
            UPDATE axon_code_projects
            SET lease_owner = ?, lease_expires_at_ms = ?
            WHERE project_key = ?
              AND (lease_owner IS NULL OR lease_owner = ? OR lease_expires_at_ms <= ?)
            "#,
        )
        .bind(owner)
        .bind(expires)
        .bind(&identity.project_key)
        .bind(owner)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub(crate) async fn release_lease(
        &self,
        identity: &CodeIndexIdentity,
        owner: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE axon_code_projects
            SET lease_owner = NULL, lease_expires_at_ms = 0
            WHERE project_key = ? AND lease_owner = ?
            "#,
        )
        .bind(&identity.project_key)
        .bind(owner)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn next_generation(
        &self,
        identity: &CodeIndexIdentity,
    ) -> anyhow::Result<i64> {
        self.upsert_project(identity).await?;
        let current: Option<(i64, i64)> = sqlx::query_as(
            r#"
            SELECT committed_generation, max_generation
            FROM axon_code_projects
            WHERE project_key = ?
            "#,
        )
        .bind(&identity.project_key)
        .fetch_optional(&self.pool)
        .await?;
        let next = current
            .map(|(committed_generation, max_generation)| {
                committed_generation.max(max_generation) + 1
            })
            .unwrap_or(1);
        sqlx::query(
            r#"
            UPDATE axon_code_projects
            SET max_generation = ?
            WHERE project_key = ?
            "#,
        )
        .bind(next)
        .bind(&identity.project_key)
        .execute(&self.pool)
        .await?;
        Ok(next)
    }

    pub async fn committed_generation(
        &self,
        identity: &CodeIndexIdentity,
    ) -> anyhow::Result<Option<i64>> {
        self.upsert_project(identity).await?;
        let current: Option<(i64,)> = sqlx::query_as(
            "SELECT committed_generation FROM axon_code_projects WHERE project_key = ?",
        )
        .bind(&identity.project_key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(current
            .map(|(generation,)| generation)
            .filter(|generation| *generation > 0))
    }

    pub(crate) async fn mark_file_pending(
        &self,
        identity: &CodeIndexIdentity,
        relative_path: &str,
    ) -> anyhow::Result<()> {
        self.upsert_project(identity).await?;
        sqlx::query(
            r#"
            UPDATE axon_code_files
            SET pending = 1, updated_at_ms = ?
            WHERE project_key = ? AND relative_path = ?
            "#,
        )
        .bind(now_ms())
        .bind(&identity.project_key)
        .bind(relative_path)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn mark_file_indexed(
        &self,
        identity: &CodeIndexIdentity,
        entry: &FileManifestEntry,
        generation: i64,
    ) -> anyhow::Result<()> {
        self.upsert_project(identity).await?;
        let hash = entry.hash.as_deref().unwrap_or_default();
        sqlx::query(
            r#"
            INSERT INTO axon_code_files
              (project_key, relative_path, hash, size_bytes, mtime_ns, indexed_generation, pending, updated_at_ms)
            VALUES (?, ?, ?, ?, ?, ?, 0, ?)
            ON CONFLICT(project_key, relative_path) DO UPDATE SET
              hash = excluded.hash,
              size_bytes = excluded.size_bytes,
              mtime_ns = excluded.mtime_ns,
              indexed_generation = excluded.indexed_generation,
              pending = 0,
              updated_at_ms = excluded.updated_at_ms
            "#,
        )
        .bind(&identity.project_key)
        .bind(&entry.relative_path)
        .bind(hash)
        .bind(entry.size_bytes as i64)
        .bind(entry.mtime_ns)
        .bind(generation)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn remove_file(
        &self,
        identity: &CodeIndexIdentity,
        relative_path: &str,
    ) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM axon_code_files WHERE project_key = ? AND relative_path = ?")
            .bind(&identity.project_key)
            .bind(relative_path)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn commit_generation(
        &self,
        identity: &CodeIndexIdentity,
        generation: i64,
    ) -> anyhow::Result<()> {
        self.upsert_project(identity).await?;
        sqlx::query(
            r#"
            UPDATE axon_code_projects
            SET committed_generation = ?, max_generation = MAX(max_generation, ?), last_checked_at_ms = ?
            WHERE project_key = ?
            "#,
        )
        .bind(generation)
        .bind(generation)
        .bind(now_ms())
        .bind(&identity.project_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn touch_last_checked(
        &self,
        identity: &CodeIndexIdentity,
    ) -> anyhow::Result<()> {
        self.upsert_project(identity).await?;
        sqlx::query(
            r#"
            UPDATE axon_code_projects
            SET last_checked_at_ms = ?
            WHERE project_key = ?
            "#,
        )
        .bind(now_ms())
        .bind(&identity.project_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn commit_manifest(
        &self,
        identity: &CodeIndexIdentity,
        manifest: &ManifestSnapshot,
    ) -> anyhow::Result<()> {
        let generation = self.next_generation(identity).await?;
        for entry in &manifest.files {
            self.mark_file_indexed(identity, entry, generation).await?;
        }
        self.commit_generation(identity, generation).await?;
        Ok(())
    }

    async fn upsert_project(&self, identity: &CodeIndexIdentity) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO axon_code_projects
              (project_key, project_display, project_root, collection, embedder_key, index_version, committed_generation, max_generation, last_checked_at_ms)
            VALUES (?, ?, ?, ?, ?, ?, 0, 0, ?)
            ON CONFLICT(project_key) DO UPDATE SET
              project_display = excluded.project_display,
              project_root = excluded.project_root,
              collection = excluded.collection,
              embedder_key = excluded.embedder_key,
              index_version = excluded.index_version
            "#,
        )
        .bind(&identity.project_key)
        .bind(&identity.project_display)
        .bind(identity.project_root.to_string_lossy().as_ref())
        .bind(&identity.collection)
        .bind(&identity.embedder_key)
        .bind(identity.index_version as i64)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn files_for_project(
        &self,
        identity: &CodeIndexIdentity,
    ) -> anyhow::Result<HashMap<String, StoredFile>> {
        let rows = sqlx::query(
            r#"
            SELECT relative_path, hash, size_bytes, mtime_ns, indexed_generation, pending
            FROM axon_code_files
            WHERE project_key = ?
            "#,
        )
        .bind(&identity.project_key)
        .fetch_all(&self.pool)
        .await?;

        let mut files = HashMap::new();
        for row in rows {
            files.insert(
                row.get::<String, _>("relative_path"),
                StoredFile {
                    hash: row.get::<String, _>("hash"),
                    size_bytes: row.get::<i64, _>("size_bytes") as u64,
                    mtime_ns: row.get::<i64, _>("mtime_ns"),
                    indexed_generation: row.get::<i64, _>("indexed_generation"),
                    pending: row.get::<i64, _>("pending") != 0,
                },
            );
        }
        Ok(files)
    }
}

pub(super) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
