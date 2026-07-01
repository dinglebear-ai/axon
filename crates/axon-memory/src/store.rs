//! Memory store boundary and in-memory fake.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use tokio::sync::Mutex;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn remember(&self, request: MemoryRequest) -> Result<MemoryResult>;
    async fn get(&self, memory_id: MemoryId) -> Result<Option<MemoryRecord>>;
    async fn search(&self, request: MemorySearchRequest) -> Result<MemorySearchResult>;
    async fn context(&self, request: MemoryContextRequest) -> Result<MemoryContextResult>;
    async fn link(&self, request: MemoryLinkRequest) -> Result<MemoryResult>;
    async fn reinforce(
        &self,
        memory_id: MemoryId,
        signal: MemoryReinforcement,
    ) -> Result<MemoryResult>;
    async fn reset(&self) -> Result<()>;
    async fn capabilities(&self) -> Result<MemoryStoreCapability>;
}

#[derive(Debug, Clone, Default)]
pub struct FakeMemoryStore {
    state: Arc<Mutex<FakeMemoryState>>,
}

#[derive(Debug, Default)]
struct FakeMemoryState {
    next_id: u64,
    records: BTreeMap<MemoryId, MemoryRecord>,
}

impl FakeMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl MemoryStore for FakeMemoryStore {
    async fn remember(&self, request: MemoryRequest) -> Result<MemoryResult> {
        let mut state = self.state.lock().await;
        state.next_id += 1;
        let memory_id = MemoryId::new(format!("mem_{}", state.next_id));
        let record = MemoryRecord {
            memory_id: memory_id.clone(),
            memory_type: request.memory_type,
            status: MemoryStatus::Active,
            body: request.body,
            confidence: request.confidence,
            salience: request.salience,
            scope: request.scope,
            history: vec![MemoryHistoryEvent {
                status: MemoryStatus::Active,
                message: "created".to_string(),
                timestamp: timestamp(),
            }],
            title: request.title,
            links: request.links,
            decay: request.decay,
            embedding_refs: Vec::new(),
        };
        let result = result_from_record(&record, memory_score(&record));
        state.records.insert(memory_id, record);
        Ok(result)
    }

    async fn get(&self, memory_id: MemoryId) -> Result<Option<MemoryRecord>> {
        Ok(self.state.lock().await.records.get(&memory_id).cloned())
    }

    async fn search(&self, request: MemorySearchRequest) -> Result<MemorySearchResult> {
        if !request.filters.is_empty() {
            return Err(unsupported_option("filters"));
        }
        if request.include_graph {
            return Err(unsupported_option("include_graph"));
        }
        if request.reinforce {
            return Err(unsupported_option("reinforce"));
        }
        let query = request.query.to_lowercase();
        let mut results = self
            .state
            .lock()
            .await
            .records
            .values()
            .filter(|record| request.include_archived || record.status == MemoryStatus::Active)
            .filter(|record| {
                record.body.to_lowercase().contains(&query) || query_matches(record, &query)
            })
            .map(|record| MemorySearchMatch {
                record: record.clone(),
                score: memory_score(record),
            })
            .collect::<Vec<_>>();
        results.sort_by(|a, b| b.score.total_cmp(&a.score));
        results.truncate(request.limit as usize);
        Ok(MemorySearchResult {
            results,
            query_embedding_model: Some("fake-memory".to_string()),
            graph: None,
            warnings: Vec::new(),
        })
    }

    async fn context(&self, request: MemoryContextRequest) -> Result<MemoryContextResult> {
        if request.source_id.is_some() {
            return Err(unsupported_option("source_id"));
        }
        if request.graph_node_id.is_some() {
            return Err(unsupported_option("graph_node_id"));
        }
        if !request.filters.is_empty() {
            return Err(unsupported_option("filters"));
        }
        if request.depth.is_some() {
            return Err(unsupported_option("depth"));
        }
        if request.include_working {
            return Err(unsupported_option("include_working"));
        }
        let query = request.query.unwrap_or_default();
        let search = self
            .search(MemorySearchRequest {
                query,
                limit: 10,
                filters: request.filters,
                include_graph: false,
                include_archived: false,
                reinforce: false,
            })
            .await?;
        let memories = search
            .results
            .into_iter()
            .map(|item| item.record)
            .collect::<Vec<_>>();
        let mut context = memories
            .iter()
            .map(|record| record.body.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let mut exclusions = Vec::new();
        let token_estimate = context.split_whitespace().count() as u32;
        if token_estimate > request.token_budget {
            context = context
                .split_whitespace()
                .take(request.token_budget as usize)
                .collect::<Vec<_>>()
                .join(" ");
            exclusions.push("token_budget".to_string());
        }
        Ok(MemoryContextResult {
            token_estimate: context.split_whitespace().count() as u32,
            context,
            memories,
            exclusions,
            warnings: Vec::new(),
        })
    }

    async fn link(&self, request: MemoryLinkRequest) -> Result<MemoryResult> {
        let mut state = self.state.lock().await;
        let record = state
            .records
            .get_mut(&request.memory_id)
            .ok_or_else(|| missing_memory(&request.memory_id))?;
        record.links.push(request.link);
        Ok(result_from_record(record, memory_score(record)))
    }

    async fn reinforce(
        &self,
        memory_id: MemoryId,
        signal: MemoryReinforcement,
    ) -> Result<MemoryResult> {
        let mut state = self.state.lock().await;
        let record = state
            .records
            .get_mut(&memory_id)
            .ok_or_else(|| missing_memory(&memory_id))?;
        record.salience = (record.salience + signal.amount).min(1.0);
        record.history.push(MemoryHistoryEvent {
            status: record.status,
            message: signal.reason,
            timestamp: signal.timestamp,
        });
        Ok(result_from_record(record, memory_score(record)))
    }

    async fn capabilities(&self) -> Result<MemoryStoreCapability> {
        Ok(CapabilityBase {
            name: "fake-memory".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-memory".to_string(),
            health: HealthStatus::Healthy,
            features: vec![
                "remember".to_string(),
                "search".to_string(),
                "context".to_string(),
                "reinforce".to_string(),
            ],
            limits: MetadataMap::new(),
        }
        .into())
    }

    async fn reset(&self) -> Result<()> {
        *self.state.lock().await = FakeMemoryState::default();
        Ok(())
    }
}

fn result_from_record(record: &MemoryRecord, memory_score: f32) -> MemoryResult {
    MemoryResult {
        memory_id: record.memory_id.clone(),
        memory_type: record.memory_type,
        status: record.status,
        memory_score,
        confidence: record.confidence,
        salience: record.salience,
        created_at: timestamp(),
        updated_at: timestamp(),
        graph_node_id: None,
        document_id: None,
        vector_point_ids: record.embedding_refs.clone(),
        warnings: Vec::new(),
    }
}

fn memory_score(record: &MemoryRecord) -> f32 {
    (record.confidence * 0.5 + record.salience * 0.5).clamp(0.0, 1.0)
}

fn query_matches(record: &MemoryRecord, query: &str) -> bool {
    query
        .split_whitespace()
        .any(|term| record.body.to_lowercase().contains(term))
}

fn missing_memory(memory_id: &MemoryId) -> ApiError {
    ApiError::new(
        "memory.not_found",
        axon_error::ErrorStage::Retrieving,
        format!("memory {} not found", memory_id.0),
    )
}

fn unsupported_option(option: &str) -> ApiError {
    ApiError::new(
        "memory.unsupported_option",
        axon_error::ErrorStage::Retrieving,
        format!("fake memory store does not implement option {option}"),
    )
}

fn timestamp() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}
