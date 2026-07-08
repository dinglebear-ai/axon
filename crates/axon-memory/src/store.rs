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
    async fn load_many(&self, memory_ids: Vec<MemoryId>) -> Result<Vec<Option<MemoryRecord>>> {
        let mut records = Vec::with_capacity(memory_ids.len());
        for memory_id in memory_ids {
            records.push(self.get(memory_id).await?);
        }
        Ok(records)
    }
    async fn search(&self, request: MemorySearchRequest) -> Result<MemorySearchResult>;
    async fn context(&self, request: MemoryContextRequest) -> Result<MemoryContextResult>;
    async fn link(&self, request: MemoryLinkRequest) -> Result<MemoryResult>;
    async fn reinforce(
        &self,
        memory_id: MemoryId,
        signal: MemoryReinforcement,
    ) -> Result<MemoryResult>;

    /// Replace `memory_id` with `replacement_id`: mark the old memory
    /// `superseded`, point it at the replacement, and record history.
    async fn supersede(&self, request: MemorySupersedeRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("supersede"))
    }

    /// Flag two memories as conflicting; both transition to `contradicted` and
    /// enter the review queue.
    async fn contradict(&self, request: MemoryContradictRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("contradict"))
    }

    /// Transition a memory to a new status (archive/forget/pin/review/etc.).
    async fn set_status(&self, request: MemoryStatusRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("set_status"))
    }

    /// Return the current review queue.
    async fn review(&self, request: MemoryReviewRequest) -> Result<MemoryReviewResult> {
        let _ = request;
        Err(unsupported_option("review"))
    }

    /// Edit a memory's editable fields (body/title/type/confidence/salience/
    /// scope) in place.
    async fn update(&self, request: MemoryUpdateRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("update"))
    }

    /// Pin or unpin a memory (exempts it from decay while pinned).
    async fn pin(&self, request: MemoryPinRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("pin"))
    }

    /// Archive a memory (excluded from recall unless explicitly requested).
    async fn archive(&self, request: MemoryArchiveRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("archive"))
    }

    /// Forget a memory (never recalled again; history is preserved).
    async fn forget(&self, request: MemoryForgetRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("forget"))
    }

    /// Merge several memories into one new memory, recording provenance.
    async fn compact(&self, request: MemoryCompactRequest) -> Result<MemoryResult> {
        let _ = request;
        Err(unsupported_option("compact"))
    }

    /// Bulk-import memory records (or preview a dry-run plan).
    async fn import(&self, request: MemoryImportRequest) -> Result<MemoryImportResult> {
        let _ = request;
        Err(unsupported_option("import"))
    }

    /// Export memory records matching a scope.
    async fn export(&self, request: MemoryExportRequest) -> Result<MemoryExportResult> {
        let _ = request;
        Err(unsupported_option("export"))
    }

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
    next_tick: u64,
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
        let created_at = state.timestamp();
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
                timestamp: created_at,
            }],
            title: request.title,
            links: request.links,
            decay: request.decay,
            embedding_refs: Vec::new(),
            superseded_by: None,
            contradicts: None,
        };
        let result = result_from_record(&record, memory_score(&record));
        state.records.insert(memory_id, record);
        Ok(result)
    }

    async fn get(&self, memory_id: MemoryId) -> Result<Option<MemoryRecord>> {
        Ok(self.state.lock().await.records.get(&memory_id).cloned())
    }

    async fn load_many(&self, memory_ids: Vec<MemoryId>) -> Result<Vec<Option<MemoryRecord>>> {
        let state = self.state.lock().await;
        Ok(memory_ids
            .into_iter()
            .map(|memory_id| state.records.get(&memory_id).cloned())
            .collect())
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
        let timestamp = state.timestamp();
        let record = state
            .records
            .get_mut(&request.memory_id)
            .ok_or_else(|| missing_memory(&request.memory_id))?;
        record.links.push(request.link);
        record.history.push(MemoryHistoryEvent {
            status: record.status,
            message: "linked".to_string(),
            timestamp,
        });
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
        record.salience = (record.salience + signal.amount).clamp(0.0, 1.0);
        record.history.push(MemoryHistoryEvent {
            status: record.status,
            message: signal.reason,
            timestamp: signal.timestamp,
        });
        Ok(result_from_record(record, memory_score(record)))
    }

    async fn supersede(&self, request: MemorySupersedeRequest) -> Result<MemoryResult> {
        let mut state = self.state.lock().await;
        if !state.records.contains_key(&request.replacement_id) {
            return Err(missing_memory(&request.replacement_id));
        }
        let record = state
            .records
            .get_mut(&request.memory_id)
            .ok_or_else(|| missing_memory(&request.memory_id))?;
        record.status = MemoryStatus::Superseded;
        record.superseded_by = Some(request.replacement_id);
        record.history.push(MemoryHistoryEvent {
            status: MemoryStatus::Superseded,
            message: request.reason.unwrap_or_else(|| "superseded".to_string()),
            timestamp: request.timestamp,
        });
        Ok(result_from_record(record, memory_score(record)))
    }

    async fn contradict(&self, request: MemoryContradictRequest) -> Result<MemoryResult> {
        let mut state = self.state.lock().await;
        let reason = request
            .reason
            .unwrap_or_else(|| "contradicted by another memory".to_string());
        {
            let record = state
                .records
                .get_mut(&request.memory_id)
                .ok_or_else(|| missing_memory(&request.memory_id))?;
            record.status = MemoryStatus::Contradicted;
            record.contradicts = Some(request.conflicting_id.clone());
            record.history.push(MemoryHistoryEvent {
                status: MemoryStatus::Contradicted,
                message: reason.clone(),
                timestamp: request.timestamp.clone(),
            });
        }
        let conflicting = state
            .records
            .get_mut(&request.conflicting_id)
            .ok_or_else(|| missing_memory(&request.conflicting_id))?;
        conflicting.status = MemoryStatus::Contradicted;
        conflicting.contradicts = Some(request.memory_id);
        conflicting.history.push(MemoryHistoryEvent {
            status: MemoryStatus::Contradicted,
            message: reason,
            timestamp: request.timestamp,
        });
        Ok(result_from_record(conflicting, memory_score(conflicting)))
    }

    async fn set_status(&self, request: MemoryStatusRequest) -> Result<MemoryResult> {
        let mut state = self.state.lock().await;
        let record = state
            .records
            .get_mut(&request.memory_id)
            .ok_or_else(|| missing_memory(&request.memory_id))?;
        record.status = request.status;
        record.history.push(MemoryHistoryEvent {
            status: request.status,
            message: request
                .reason
                .unwrap_or_else(|| "status updated".to_string()),
            timestamp: request.timestamp,
        });
        Ok(result_from_record(record, memory_score(record)))
    }

    async fn review(&self, request: MemoryReviewRequest) -> Result<MemoryReviewResult> {
        let state = self.state.lock().await;
        let limit = request
            .limit
            .and_then(|limit| usize::try_from(limit).ok())
            .filter(|limit| *limit > 0)
            .unwrap_or(usize::MAX);
        let mut skipped_cursor = request.cursor.is_none();
        let mut memories = Vec::new();
        let mut next_cursor = None;
        let mut last_returned_id = None;

        for (memory_id, record) in &state.records {
            if !skipped_cursor {
                skipped_cursor = request.cursor.as_deref() == Some(memory_id.0.as_str());
                continue;
            }
            if request
                .memory_type
                .is_some_and(|memory_type| record.memory_type != memory_type)
            {
                continue;
            }
            if request
                .scope
                .as_ref()
                .is_some_and(|scope| &record.scope != scope)
            {
                continue;
            }
            if request.reason.as_ref().is_some_and(|reason| {
                !record
                    .history
                    .iter()
                    .any(|event| event.message.contains(reason))
            }) {
                continue;
            }
            if memories.len() >= limit {
                next_cursor = last_returned_id;
                break;
            }
            memories.push(record.clone());
            last_returned_id = Some(memory_id.0.clone());
        }

        Ok(MemoryReviewResult {
            memories,
            cursor: next_cursor,
            warnings: Vec::new(),
        })
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
    let created_at = record
        .history
        .first()
        .map(|event| event.timestamp.clone())
        .unwrap_or_else(timestamp);
    let updated_at = record
        .history
        .last()
        .map(|event| event.timestamp.clone())
        .unwrap_or_else(|| created_at.clone());
    MemoryResult {
        memory_id: record.memory_id.clone(),
        memory_type: record.memory_type,
        status: record.status,
        memory_score,
        confidence: record.confidence,
        salience: record.salience,
        created_at,
        updated_at,
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

impl FakeMemoryState {
    fn timestamp(&mut self) -> Timestamp {
        self.next_tick += 1;
        Timestamp(format!("2026-07-01T00:00:{:02}Z", self.next_tick))
    }
}
