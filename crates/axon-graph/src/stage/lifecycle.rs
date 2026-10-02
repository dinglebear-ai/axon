//! Ownership outlives queued SQLite work, including future cancellation.
use super::*;

impl Drop for GraphStage {
    fn drop(&mut self) {
        let Some(owner) = self.owner.get_mut().take() else {
            return;
        };
        let owner = std::sync::Arc::new(owner);
        let thread_owner = std::sync::Arc::clone(&owner);
        let pool = self.store.pool().clone();
        let stage_id = self.id.clone();
        let settling_id = stage_id.clone();
        // A separate settlement thread survives cancellation/shutdown of the
        // runtime that owned the stage. It performs no graph writes.
        let spawned = std::thread::Builder::new()
            .name("axon-graph-stage-settle".into())
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => {
                        runtime.block_on(settle_private_pool(&pool));
                        drop(thread_owner);
                    }
                    // Fail closed: retain ownership until process exit rather
                    // than allowing disposal while SQLite work may still run.
                    Err(error) => {
                        tracing::error!(stage_id=%settling_id,error=%error,"graph stage settlement runtime unavailable; owner lock retained until process exit, inspect disposal debt before restarting worker process");
                        std::mem::forget(thread_owner);
                    },
                }
            });
        if let Err(error) = spawned {
            tracing::error!(stage_id=%stage_id,error=%error,"graph stage settlement thread unavailable; owner lock retained until process exit, inspect disposal debt before restarting worker process");
            std::mem::forget(owner);
        }
    }
}

pub(super) async fn mark_disposable_ids(pool: &SqlitePool, ids: &[String]) -> Result<()> {
    for id in ids {
        if uuid::Uuid::parse_str(id).is_err() {
            return Err(graph_storage_error("invalid opaque graph stage ID"));
        }
        let path: Option<String> =
            sqlx::query_scalar("SELECT path FROM graph_stages WHERE stage_id=?")
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(storage::error)?;
        let Some(path) = path else {
            continue;
        };
        let path = PathBuf::from(path);
        storage::validate_disposal_path(pool, &path, id).await?;
        let _owner = match storage::owner_lock(&path) {
            Ok(owner) => owner,
            Err(error)
                if error
                    .details
                    .get("owner_active")
                    .is_some_and(|v| v == "true") =>
            {
                continue;
            }
            Err(error) => return Err(error),
        };
        sqlx::query("UPDATE graph_stages SET state='disposable' WHERE stage_id=?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(storage::error)?;
    }
    Ok(())
}

/// SQLx 0.8 can finish close early: draining an idle connection releases
/// an additional semaphore permit. Size reaches zero only after every
/// checked-out connection and its SQLite worker have closed.
pub(super) async fn settle_private_pool(pool: &SqlitePool) {
    pool.close().await;
    while pool.size() != 0 {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}
