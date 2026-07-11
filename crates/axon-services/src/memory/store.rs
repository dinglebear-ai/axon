//! Durable memory store composition.

use super::*;
use axon_memory::sqlite::compact::CompactionSynthesizer;

/// Real [`CompactionSynthesizer`] for the `compact` strategy
/// `"semantic_summary"` (contract R3-20), backed by the same `axon-llm`
/// completion boundary `summarize`/`ask`/`research` use — never a bespoke
/// HTTP client. Config-driven backend selection (`AXON_LLM_BACKEND`) is
/// inherited from `cfg` the same way `summarize::summarize` does via
/// `CompletionRequest::backend_from_config`.
struct LlmCompactionSynthesizer {
    cfg: Arc<axon_core::config::Config>,
}

#[async_trait::async_trait]
impl CompactionSynthesizer for LlmCompactionSynthesizer {
    async fn synthesize(
        &self,
        sources: &[axon_api::source::MemoryRecord],
        instructions: Option<&str>,
    ) -> std::result::Result<String, String> {
        let joined = sources
            .iter()
            .map(|record| format!("[{}] {}", record.memory_id.0, record.body))
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut prompt = format!(
            "Distill the following memories into one concise, factual summary. \
             Do not invent facts not present in the source memories.\n\n{joined}"
        );
        if let Some(instructions) = instructions {
            prompt.push_str(&format!("\n\nAdditional instructions: {instructions}"));
        }
        let request = axon_llm::CompletionRequest::new(prompt)
            .system_prompt(
                "You distill multiple durable memory records into a single, concise, \
                 non-fabricating summary for long-term storage.",
            )
            .effort(axon_llm::ReasoningEffort::Low)
            .backend_from_config(&self.cfg);
        let completion = axon_llm::complete_text(request)
            .await
            .map_err(|error| error.to_string())?;
        Ok(completion.text.trim().to_string())
    }
}

/// Open the durable SQLite memory store against the unified jobs DB.
///
/// The memory tables (`memory_records`/`memory_links`/…) are created by the
/// composed cross-crate migration runner on the same DB file at startup;
/// `SqliteMemoryStore::open` also runs the idempotent in-crate schema, so it is
/// safe to open here regardless of composition order. The graph mirror opens
/// its own pool against the same path (`SqliteGraphStore::connect` runs its
/// own idempotent `ensure_schema`) — safe alongside the sync rusqlite handle
/// SQLite memory store holds, same file, different table set.
pub(crate) async fn memory_store(ctx: &ServiceContext) -> Result<Arc<dyn MemoryStore>> {
    let path = ctx.cfg().sqlite_path.to_string_lossy().to_string();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let sqlite: Arc<dyn MemoryStore> = Arc::new(
        SqliteMemoryStore::open(&path, clock)
            .map_err(|e| anyhow::anyhow!("open memory store at {path}: {}", e.message))?
            .with_compaction_synthesizer(Arc::new(LlmCompactionSynthesizer {
                cfg: Arc::clone(&ctx.cfg),
            })),
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
