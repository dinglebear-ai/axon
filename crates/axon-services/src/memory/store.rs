//! Durable memory store composition.

use super::*;

/// Open the durable SQLite memory store against the unified jobs DB.
///
/// The memory tables (`memory_records`/`memory_links`/…) are created by the
/// composed cross-crate migration runner on the same DB file at startup;
/// `SqliteMemoryStore::open` also runs the idempotent in-crate schema, so it is
/// safe to open here regardless of composition order. The graph mirror opens
/// its own pool against the same path (`SqliteGraphStore::connect` runs its
/// own idempotent `ensure_schema`) — safe alongside the sync rusqlite handle
/// SQLite memory store holds, same file, different table set.
pub(super) async fn memory_store(ctx: &ServiceContext) -> Result<Arc<dyn MemoryStore>> {
    let path = ctx.cfg().sqlite_path.to_string_lossy().to_string();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let sqlite: Arc<dyn MemoryStore> = Arc::new(
        SqliteMemoryStore::open(&path, clock)
            .map_err(|e| anyhow::anyhow!("open memory store at {path}: {}", e.message))?,
    );
    let graph = SqliteGraphStore::connect(&path)
        .await
        .map_err(|e| anyhow::anyhow!("open memory graph mirror at {path}: {}", e.message))?;
    let mirror = Arc::new(GraphBackedMemoryMirror::new(Arc::new(graph)));
    let sqlite: Arc<dyn MemoryStore> = Arc::new(GraphBackedMemoryStore::new(sqlite, mirror));
    let Some(runtime) = ctx.target_local_source_runtime() else {
        return Ok(sqlite);
    };
    Ok(Arc::new(VectorBackedMemoryStore::new(
        sqlite,
        Arc::clone(&runtime.embedding_provider),
        Arc::clone(&runtime.vector_store),
        MemoryVectorConfig {
            collection: ctx.cfg().collection.clone(),
            embedding_provider_id: runtime.embedding_provider_id.clone(),
            embedding_model: runtime.embedding_model.clone(),
            embedding_dimensions: runtime.embedding_dimensions,
            batch_limits: MemoryBatchLimits::default(),
        },
    )))
}
