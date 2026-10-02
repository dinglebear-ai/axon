//! Durable, private graph construction and atomic publication.
mod lifecycle;
mod redaction;
mod storage;
use crate::{SqliteGraphStore, error::graph_storage_error};
use axon_api::source::{GraphCandidate, GraphWriteResult, JobId, SourceGenerationId, SourceId};
use sqlx::{SqliteConnection, SqlitePool};
use std::path::PathBuf;
use tokio::sync::Mutex;
type Result<T> = std::result::Result<T, axon_api::source::ApiError>;
const LIMIT: u64 = 256 * 1024 * 1024;

pub struct GraphStage {
    id: String,
    live: SqlitePool,
    store: SqliteGraphStore,
    path: PathBuf,
    source: SourceId,
    generation: SourceGenerationId,
    job: JobId,
    attempt: u32,
    baseline: i64,
    live_memory: bool,
    gate: Mutex<()>,
    sealed: std::sync::atomic::AtomicBool,
    owner: Mutex<Option<std::fs::File>>,
}
impl GraphStage {
    pub async fn begin(
        live: SqlitePool,
        source: SourceId,
        generation: SourceGenerationId,
        job: JobId,
        attempt: u32,
    ) -> Result<Self> {
        let id = uuid::Uuid::new_v4().to_string();
        let main: String =
            sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name='main'")
                .fetch_one(&live)
                .await
                .map_err(storage::error)?;
        let live_memory = main.is_empty();
        let path = storage::stage_path(&live, &id).await?;
        let baseline = sqlx::query_scalar("SELECT revision FROM graph_revision WHERE singleton=1")
            .fetch_one(&live)
            .await
            .map_err(storage::error)?;
        sqlx::query("INSERT INTO graph_stages(stage_id,source_id,generation_id,job_id,attempt,path) VALUES(?,?,?,?,?,?)")
            .bind(&id).bind(&source.0).bind(&generation.0).bind(job.0.to_string()).bind(attempt).bind(path.to_string_lossy().as_ref()).execute(&live).await.map_err(storage::error)?;
        let owner = storage::owner_lock(&path)?;
        storage::create_private_file(&path)?;
        let store = SqliteGraphStore::connect(
            path.to_str()
                .ok_or_else(|| graph_storage_error("graph stage path is not UTF-8"))?,
        )
        .await?;
        storage::disable_private_revision_triggers(store.pool()).await?;
        sqlx::query("CREATE TABLE stage_journal(sequence INTEGER PRIMARY KEY, candidate_json TEXT NOT NULL)").execute(store.pool()).await.map_err(storage::error)?;
        sqlx::query("CREATE TABLE stage_stats(singleton INTEGER PRIMARY KEY, bytes INTEGER NOT NULL, journal_bytes INTEGER NOT NULL, summary_json TEXT NOT NULL)").execute(store.pool()).await.map_err(storage::error)?;
        sqlx::query("INSERT INTO stage_stats VALUES(1,0,0,?)")
            .bind(
                serde_json::to_string(&storage::summary(&source, &[]))
                    .map_err(|_| graph_storage_error("graph stage summary serialization failed"))?,
            )
            .execute(store.pool())
            .await
            .map_err(storage::error)?;
        Ok(Self {
            id,
            live,
            store,
            path,
            source,
            generation,
            job,
            attempt,
            baseline,
            live_memory,
            gate: Mutex::new(()),
            sealed: std::sync::atomic::AtomicBool::new(false),
            owner: Mutex::new(Some(owner)),
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub async fn write_candidates(
        &self,
        candidates: Vec<GraphCandidate>,
    ) -> Result<GraphWriteResult> {
        self.write_candidates_inner(candidates)
            .await
            .map_err(|e| self.context(e, "private_stage_uncommitted"))
    }
    async fn write_candidates_inner(
        &self,
        candidates: Vec<GraphCandidate>,
    ) -> Result<GraphWriteResult> {
        let _guard = self.gate.lock().await;
        if self.sealed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(graph_storage_error(
                "graph stage is immutable; create a new attempt",
            ));
        }
        let state: String = sqlx::query_scalar("SELECT state FROM graph_stages WHERE stage_id=?")
            .bind(&self.id)
            .fetch_one(&self.live)
            .await
            .map_err(storage::error)?;
        if state != "building" {
            return Err(graph_storage_error(
                "graph stage is immutable; create a new attempt",
            ));
        }
        let candidates = candidates
            .into_iter()
            .map(redaction::sanitize)
            .collect::<Result<Vec<_>>>()?;
        let mut encoded = Vec::new();
        for candidate in &candidates {
            crate::candidate::validate_candidate(candidate)?;
            if candidate.source_id != self.source {
                return Err(graph_storage_error(
                    "graph stage candidate source differs from owner",
                ));
            }
            encoded
                .push(serde_json::to_string(candidate).map_err(|_| {
                    graph_storage_error("graph stage journal serialization failed")
                })?);
        }
        let bytes: i64 = sqlx::query_scalar("SELECT bytes FROM stage_stats WHERE singleton=1")
            .fetch_one(self.store.pool())
            .await
            .map_err(storage::error)?;
        if bytes as u64 + encoded.iter().map(|s| s.len() as u64).sum::<u64>() > LIMIT {
            return Err(graph_storage_error(
                "graph journal and generation side effects exceed combined 256 MiB budget; reduce generation size and retry",
            ));
        }
        storage::seed(&self.live, self.store.pool(), &candidates).await?;
        storage::check_size(&self.path)?;
        let mut tx = self.store.pool().begin().await.map_err(storage::error)?;
        let added_bytes: i64 = encoded.iter().map(|s| s.len() as i64).sum();
        for json in encoded {
            sqlx::query("INSERT INTO stage_journal(candidate_json) VALUES(?)")
                .bind(json)
                .execute(&mut *tx)
                .await
                .map_err(storage::error)?;
        }
        let result = crate::sqlite::upsert::replay_in_tx(&mut tx, &candidates).await?;
        let previous: String =
            sqlx::query_scalar("SELECT summary_json FROM stage_stats WHERE singleton=1")
                .fetch_one(&mut *tx)
                .await
                .map_err(storage::error)?;
        let mut summary: GraphWriteResult = serde_json::from_str(&previous)
            .map_err(|_| graph_storage_error("graph stage summary corrupt"))?;
        summary.candidates_seen += result.candidates_seen;
        summary.nodes_upserted += result.nodes_upserted;
        summary.edges_upserted += result.edges_upserted;
        summary.evidence_records += result.evidence_records;
        sqlx::query("UPDATE stage_stats SET bytes=bytes+?1,journal_bytes=journal_bytes+?1,summary_json=?2 WHERE singleton=1")
            .bind(added_bytes)
            .bind(
                serde_json::to_string(&summary)
                    .map_err(|_| graph_storage_error("graph stage summary serialization failed"))?,
            )
            .execute(&mut *tx)
            .await
            .map_err(storage::error)?;
        tx.commit().await.map_err(storage::error)?;
        storage::check_size(&self.path)?;
        Ok(result)
    }
    pub async fn prepare_activation(&self, conn: &mut SqliteConnection) -> Result<()> {
        self.seal_on(conn).await?;
        if self.live_memory {
            return Ok(());
        }
        sqlx::query("ATTACH DATABASE ? AS axon_stage")
            .bind(format!("file:{}?mode=ro", self.path.to_string_lossy()))
            .execute(conn)
            .await
            .map_err(storage::error)?;
        Ok(())
    }
    pub async fn detach(&self, conn: &mut SqliteConnection) -> Result<()> {
        if self.live_memory {
            return Ok(());
        }
        sqlx::query("DETACH DATABASE axon_stage")
            .execute(conn)
            .await
            .map_err(storage::error)?;
        Ok(())
    }
    pub async fn activate_in_tx(&self, conn: &mut SqliteConnection) -> Result<GraphWriteResult> {
        self.activate_inner(conn)
            .await
            .map_err(|e| self.context(e, "caller_transaction_uncommitted"))
    }
    async fn activate_inner(&self, conn: &mut SqliteConnection) -> Result<GraphWriteResult> {
        let _guard = self.gate.lock().await;
        let receipt: Option<String> =
            sqlx::query_scalar("SELECT summary_json FROM graph_stage_receipts WHERE stage_id=?")
                .bind(&self.id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(storage::error)?;
        if let Some(receipt) = receipt {
            return serde_json::from_str(&receipt).map_err(|_| {
                graph_storage_error("graph stage receipt corrupt; inspect stage ownership")
            });
        }
        let state: String = sqlx::query_scalar("SELECT state FROM graph_stages WHERE stage_id=?")
            .bind(&self.id)
            .fetch_one(&mut *conn)
            .await
            .map_err(storage::error)?;
        if state != "ready" {
            return Err(graph_storage_error(
                "graph stage disposed; retry with a new attempt",
            ));
        }
        let revision: i64 =
            sqlx::query_scalar("SELECT revision FROM graph_revision WHERE singleton=1")
                .fetch_one(&mut *conn)
                .await
                .map_err(storage::error)?;
        let replay = self.live_memory || revision != self.baseline;
        let journal_bytes: i64 =
            sqlx::query_scalar("SELECT journal_bytes FROM stage_stats WHERE singleton=1")
                .fetch_one(self.store.pool())
                .await
                .map_err(storage::error)?;
        let started = std::time::Instant::now();
        let mut result = if replay {
            storage::replay_journal(self.store.pool(), conn, &self.source).await?
        } else {
            storage::bulk_apply(conn).await?;
            self.summary().await?
        };
        tracing::info!(stage_id=%self.id,source_id=%self.source.0,job_id=%self.job.0,
            generation_id=%self.generation.0,attempt=self.attempt,replay,
            activation_strategy=if replay {"replay"} else {"bulk"},
            fallback_candidates=if replay {result.candidates_seen} else {0},journal_bytes,
            activation_elapsed_ms=started.elapsed().as_millis() as u64,
            "private graph activation prepared; caller transaction remains uncommitted");
        result.source_id = self.source.clone();
        let json = serde_json::to_string(&result)
            .map_err(|_| graph_storage_error("graph stage receipt serialization failed"))?;
        sqlx::query("INSERT INTO graph_stage_receipts(stage_id,source_id,generation_id,summary_json) SELECT stage_id,source_id,generation_id,? FROM graph_stages WHERE stage_id=? AND state IN ('building','ready')").bind(&json).bind(&self.id).execute(&mut *conn).await.map_err(storage::error)?;
        sqlx::query("UPDATE graph_stages SET state='activated',summary_json=? WHERE stage_id=? AND state IN ('building','ready')").bind(json).bind(&self.id).execute(conn).await.map_err(storage::error)?;
        Ok(result)
    }
    pub async fn summary(&self) -> Result<GraphWriteResult> {
        let json: String =
            sqlx::query_scalar("SELECT summary_json FROM stage_stats WHERE singleton=1")
                .fetch_one(self.store.pool())
                .await
                .map_err(storage::error)?;
        serde_json::from_str(&json).map_err(|_| graph_storage_error("graph stage summary corrupt"))
    }
    pub async fn mark_disposable(&self) -> Result<()> {
        let _guard = self.gate.lock().await;
        sqlx::query("UPDATE graph_stages SET state='disposable' WHERE stage_id=?")
            .bind(&self.id)
            .execute(&self.live)
            .await
            .map_err(storage::error)?;
        self.sealed
            .store(true, std::sync::atomic::Ordering::Release);
        self.store.pool().close().await;
        self.owner.lock().await.take();
        Ok(())
    }
    pub async fn reap(pool: &SqlitePool, ids: &[String]) -> Result<usize> {
        storage::reap(pool, ids).await
    }
}

/// Durable stage identity for job-aware recovery; private paths stay in graph storage.
#[derive(Debug, Clone)]
pub struct GraphStageRecord {
    pub stage_id: String,
    pub source_id: SourceId,
    pub generation_id: SourceGenerationId,
    pub job_id: JobId,
    pub attempt: u32,
    pub state: String,
}
impl GraphStage {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<GraphStageRecord>> {
        use sqlx::Row;
        let rows = sqlx::query(
            "SELECT stage_id,source_id,generation_id,job_id,attempt,state FROM graph_stages ORDER BY CASE WHEN state='disposable' THEN 0 WHEN state='activated' THEN 1 ELSE 2 END,created_at LIMIT 1000",
        )
        .fetch_all(pool)
        .await
        .map_err(storage::error)?;
        rows.into_iter()
            .map(|r| {
                Ok(GraphStageRecord {
                    stage_id: r.get("stage_id"),
                    source_id: SourceId::new(r.get::<String, _>("source_id")),
                    generation_id: SourceGenerationId::new(r.get::<String, _>("generation_id")),
                    job_id: JobId::new(
                        uuid::Uuid::parse_str(&r.get::<String, _>("job_id"))
                            .map_err(|_| graph_storage_error("graph stage owner job ID corrupt"))?,
                    ),
                    attempt: r.get("attempt"),
                    state: r.get("state"),
                })
            })
            .collect()
    }
    /// Caller must establish that the owning job/attempt has settled before eligibility.
    pub async fn mark_disposable_ids(pool: &SqlitePool, ids: &[String]) -> Result<()> {
        lifecycle::mark_disposable_ids(pool, ids).await
    }
}
impl GraphStage {
    pub async fn activation_summary(
        pool: &SqlitePool,
        source: &SourceId,
        generation: &SourceGenerationId,
    ) -> Result<Option<GraphWriteResult>> {
        let receipt: Option<String> = sqlx::query_scalar("SELECT summary_json FROM graph_stage_receipts WHERE source_id=? AND generation_id=? ORDER BY activated_at DESC LIMIT 1").bind(&source.0).bind(&generation.0).fetch_optional(pool).await.map_err(storage::error)?;
        receipt
            .map(|json| {
                serde_json::from_str(&json).map_err(|_| {
                    graph_storage_error(
                        "graph activation receipt corrupt; inspect generation commit state",
                    )
                })
            })
            .transpose()
    }
}

impl GraphStage {
    fn context(
        &self,
        error: axon_api::source::ApiError,
        commit_state: &str,
    ) -> axon_api::source::ApiError {
        error.with_source_id(&self.source.0).with_job_id(self.job.0.to_string()).with_context("stage_id",&self.id).with_context("generation_id",&self.generation.0).with_context("attempt",self.attempt.to_string()).with_context("commit_state",commit_state).with_context("recovery","settle stage writer, inspect durable activation receipt, then retry generation or dispose eligible stage")
    }
}

impl GraphStage {
    /// Freeze the complete stage before publication. Further writes are rejected.
    pub async fn seal(&self) -> Result<GraphWriteResult> {
        let mut conn = self.live.acquire().await.map_err(storage::error)?;
        self.seal_on(&mut conn).await
    }
    async fn seal_on(&self, conn: &mut SqliteConnection) -> Result<GraphWriteResult> {
        let _guard = self.gate.lock().await;
        let state: String = sqlx::query_scalar("SELECT state FROM graph_stages WHERE stage_id=?")
            .bind(&self.id)
            .fetch_one(&mut *conn)
            .await
            .map_err(storage::error)?;
        if state == "ready" {
            self.sealed
                .store(true, std::sync::atomic::Ordering::Release);
            return self.summary().await;
        }
        if state != "building" {
            return Err(self.context(
                graph_storage_error("graph stage cannot be sealed after disposal or activation"),
                "not_activated",
            ));
        }
        let (journal, write_set) = storage::digests(self.store.pool()).await?;
        let summary = self.summary().await?;
        sqlx::query("UPDATE graph_stages SET state='ready',summary_json=?,journal_digest=?,write_set_digest=? WHERE stage_id=? AND state='building'").bind(serde_json::to_string(&summary).map_err(|_|graph_storage_error("graph stage summary serialization failed"))?).bind(journal).bind(write_set).bind(&self.id).execute(&mut *conn).await.map_err(storage::error)?;
        self.sealed
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(summary)
    }
}

#[cfg(test)]
impl GraphStage {
    pub(crate) async fn test_acquire_private_connection(
        &self,
    ) -> sqlx::pool::PoolConnection<sqlx::Sqlite> {
        self.store.pool().acquire().await.unwrap()
    }
}

impl GraphStage {
    /// Charge non-graph generation side effects against the shared journal budget.
    pub async fn reserve_side_effect_bytes(&self, bytes: u64) -> Result<()> {
        let _guard = self.gate.lock().await;
        if self.sealed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(self.context(
                graph_storage_error(
                    "graph stage is immutable; cannot reserve generation side effects",
                ),
                "private_stage_uncommitted",
            ));
        }
        let used: i64 = sqlx::query_scalar("SELECT bytes FROM stage_stats WHERE singleton=1")
            .fetch_one(self.store.pool())
            .await
            .map_err(storage::error)?;
        if (used as u64)
            .checked_add(bytes)
            .is_none_or(|total| total > LIMIT)
        {
            return Err(self.context(graph_storage_error("graph journal and generation side effects exceed combined 256 MiB budget; reduce generation size and retry"),"private_stage_uncommitted"));
        }
        sqlx::query("UPDATE stage_stats SET bytes=bytes+? WHERE singleton=1")
            .bind(bytes as i64)
            .execute(self.store.pool())
            .await
            .map_err(storage::error)?;
        Ok(())
    }
}
