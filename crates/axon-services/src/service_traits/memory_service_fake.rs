use std::sync::Mutex;

use async_trait::async_trait;
use axon_api::source::{
    MemoryArchiveRequest, MemoryCompactRequest, MemoryContextRequest, MemoryContextResult,
    MemoryContradictRequest, MemoryId, MemoryLinkRequest, MemoryPinRequest, MemoryRecord,
    MemoryReinforcement, MemoryRequest, MemoryResult, MemoryReviewRequest, MemoryReviewResult,
    MemorySearchRequest, MemorySearchResult, MemoryStatus, MemorySupersedeRequest,
    MemoryUpdateRequest, Visibility,
};

use super::MemoryService;

fn fake_record(memory_id: MemoryId, request: &MemoryRequest) -> MemoryRecord {
    MemoryRecord {
        memory_id,
        memory_type: request.memory_type,
        status: MemoryStatus::Active,
        body: request.body.clone(),
        confidence: request.confidence,
        salience: request.salience,
        scope: request.scope.clone(),
        history: Vec::new(),
        visibility: request.visibility.unwrap_or(Visibility::Internal),
        title: request.title.clone(),
        links: request.links.clone(),
        decay: request.decay.clone(),
        embedding_refs: Vec::new(),
        superseded_by: None,
        contradicts: None,
    }
}

fn record_to_result(record: &MemoryRecord) -> MemoryResult {
    let now = axon_api::source::Timestamp::from(chrono::Utc::now());
    MemoryResult {
        memory_id: record.memory_id.clone(),
        memory_type: record.memory_type,
        status: record.status,
        memory_score: record.confidence * record.salience,
        confidence: record.confidence,
        salience: record.salience,
        created_at: now.clone(),
        updated_at: now,
        graph_node_id: None,
        document_id: None,
        vector_point_ids: Vec::new(),
        warnings: Vec::new(),
    }
}

/// Deterministic in-memory fake covering every `MemoryService` method.
#[derive(Default)]
pub struct FakeMemoryService {
    records: Mutex<std::collections::HashMap<String, MemoryRecord>>,
}

impl FakeMemoryService {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl MemoryService for FakeMemoryService {
    async fn remember(&self, request: MemoryRequest) -> anyhow::Result<MemoryResult> {
        let memory_id = MemoryId::new(format!("memory-{}", uuid::Uuid::new_v4()));
        let record = fake_record(memory_id, &request);
        let result = record_to_result(&record);
        self.records
            .lock()
            .unwrap()
            .insert(result.memory_id.0.clone(), record);
        Ok(result)
    }

    async fn get(&self, memory_id: MemoryId) -> anyhow::Result<MemoryRecord> {
        self.records
            .lock()
            .unwrap()
            .get(&memory_id.0)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("memory {} not found", memory_id.0))
    }

    async fn search(&self, request: MemorySearchRequest) -> anyhow::Result<MemorySearchResult> {
        let records = self.records.lock().unwrap();
        let results = records
            .values()
            .filter(|record| record.body.contains(&request.query))
            .take(request.limit as usize)
            .map(|record| axon_api::source::MemorySearchMatch {
                record: record.clone(),
                score: 1.0,
            })
            .collect();
        Ok(MemorySearchResult {
            results,
            query_embedding_model: None,
            graph: None,
            warnings: Vec::new(),
        })
    }

    async fn context(&self, request: MemoryContextRequest) -> anyhow::Result<MemoryContextResult> {
        let _ = request.token_budget;
        let records = self.records.lock().unwrap();
        let memories: Vec<MemoryRecord> = records.values().cloned().collect();
        let context = memories
            .iter()
            .map(|m| m.body.clone())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(MemoryContextResult {
            token_estimate: (context.len() / 4) as u32,
            context,
            memories,
            exclusions: Vec::new(),
            warnings: Vec::new(),
        })
    }

    async fn link(&self, request: MemoryLinkRequest) -> anyhow::Result<MemoryResult> {
        let mut records = self.records.lock().unwrap();
        let record = records
            .get_mut(&request.memory_id.0)
            .ok_or_else(|| anyhow::anyhow!("memory {} not found", request.memory_id.0))?;
        record.links.push(request.link);
        Ok(record_to_result(record))
    }

    async fn update(&self, request: MemoryUpdateRequest) -> anyhow::Result<MemoryResult> {
        let mut records = self.records.lock().unwrap();
        let record = records
            .get_mut(&request.memory_id.0)
            .ok_or_else(|| anyhow::anyhow!("memory {} not found", request.memory_id.0))?;
        if let Some(body) = request.body {
            record.body = body;
        }
        if let Some(title) = request.title {
            record.title = Some(title);
        }
        if let Some(memory_type) = request.memory_type {
            record.memory_type = memory_type;
        }
        if let Some(confidence) = request.confidence {
            record.confidence = confidence;
        }
        if let Some(salience) = request.salience {
            record.salience = salience;
        }
        if let Some(scope) = request.scope {
            record.scope = scope;
        }
        Ok(record_to_result(record))
    }

    async fn reinforce(
        &self,
        memory_id: MemoryId,
        signal: MemoryReinforcement,
    ) -> anyhow::Result<MemoryResult> {
        let mut records = self.records.lock().unwrap();
        let record = records
            .get_mut(&memory_id.0)
            .ok_or_else(|| anyhow::anyhow!("memory {} not found", memory_id.0))?;
        record.salience = (record.salience + signal.amount).clamp(0.0, 1.0);
        if let Some(decay) = record.decay.as_mut() {
            decay.reinforcement_count = decay.reinforcement_count.saturating_add(1);
            decay.last_reinforced_at = Some(signal.timestamp);
        }
        Ok(record_to_result(record))
    }

    async fn supersede(&self, request: MemorySupersedeRequest) -> anyhow::Result<MemoryResult> {
        if request.memory_id == request.replacement_id {
            anyhow::bail!("a memory cannot supersede itself");
        }
        let mut records = self.records.lock().unwrap();
        if !records.contains_key(&request.replacement_id.0) {
            anyhow::bail!("memory {} not found", request.replacement_id.0);
        }
        let record = records
            .get_mut(&request.memory_id.0)
            .ok_or_else(|| anyhow::anyhow!("memory {} not found", request.memory_id.0))?;
        record.status = MemoryStatus::Superseded;
        record.superseded_by = Some(request.replacement_id);
        Ok(record_to_result(record))
    }

    async fn contradict(&self, request: MemoryContradictRequest) -> anyhow::Result<MemoryResult> {
        if request.memory_id == request.conflicting_id {
            anyhow::bail!("a memory cannot contradict itself");
        }
        let mut records = self.records.lock().unwrap();
        for id in [&request.memory_id.0, &request.conflicting_id.0] {
            if !records.contains_key(id) {
                anyhow::bail!("memory {id} not found");
            }
        }
        for (id, other) in [
            (request.memory_id.clone(), request.conflicting_id.clone()),
            (request.conflicting_id.clone(), request.memory_id.clone()),
        ] {
            let record = records
                .get_mut(&id.0)
                .expect("presence already checked above");
            record.status = MemoryStatus::Contradicted;
            record.contradicts = Some(other);
        }
        let record = records
            .get(&request.memory_id.0)
            .expect("presence already checked above");
        Ok(record_to_result(record))
    }

    async fn pin(&self, request: MemoryPinRequest) -> anyhow::Result<MemoryResult> {
        let mut records = self.records.lock().unwrap();
        let record = records
            .get_mut(&request.memory_id.0)
            .ok_or_else(|| anyhow::anyhow!("memory {} not found", request.memory_id.0))?;
        match record.decay.as_mut() {
            Some(decay) => decay.pinned = request.pinned,
            None => {
                record.decay = Some(axon_api::source::MemoryDecayPolicy {
                    profile: "none".to_string(),
                    half_life_days: None,
                    last_reinforced_at: None,
                    reinforcement_count: 0,
                    review_after: None,
                    expires_at: None,
                    pinned: request.pinned,
                });
            }
        }
        Ok(record_to_result(record))
    }

    async fn archive(&self, request: MemoryArchiveRequest) -> anyhow::Result<MemoryResult> {
        let mut records = self.records.lock().unwrap();
        let record = records
            .get_mut(&request.memory_id.0)
            .ok_or_else(|| anyhow::anyhow!("memory {} not found", request.memory_id.0))?;
        record.status = MemoryStatus::Archived;
        Ok(record_to_result(record))
    }

    async fn forget(&self, memory_id: MemoryId) -> anyhow::Result<MemoryResult> {
        let mut records = self.records.lock().unwrap();
        let record = records
            .get_mut(&memory_id.0)
            .ok_or_else(|| anyhow::anyhow!("memory {} not found", memory_id.0))?;
        record.status = MemoryStatus::Forgotten;
        Ok(record_to_result(record))
    }

    async fn review(&self, request: MemoryReviewRequest) -> anyhow::Result<MemoryReviewResult> {
        let records = self.records.lock().unwrap();
        let limit = request.limit.unwrap_or(50).max(1) as usize;
        let memories: Vec<MemoryRecord> = records
            .values()
            .filter(|record| {
                matches!(
                    record.status,
                    MemoryStatus::Review | MemoryStatus::Contradicted
                )
            })
            .filter(|record| request.memory_type.is_none_or(|t| record.memory_type == t))
            .take(limit)
            .cloned()
            .collect();
        Ok(MemoryReviewResult {
            memories,
            cursor: None,
            warnings: Vec::new(),
        })
    }

    async fn compact(&self, request: MemoryCompactRequest) -> anyhow::Result<MemoryResult> {
        if request.memory_ids.len() < 2 {
            anyhow::bail!("compact requires at least 2 source memories");
        }
        if request.strategy != "concatenate" {
            anyhow::bail!(
                "compact strategy {:?} is not implemented in the fake; only \"concatenate\" is \
                 supported",
                request.strategy
            );
        }
        let mut records = self.records.lock().unwrap();
        let mut sources = Vec::with_capacity(request.memory_ids.len());
        for id in &request.memory_ids {
            let record = records
                .get(&id.0)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("memory {} not found", id.0))?;
            sources.push(record);
        }
        let body = sources
            .iter()
            .map(|record| format!("[{}] {}", record.memory_id.0, record.body))
            .collect::<Vec<_>>()
            .join("\n\n");
        let memory_id = MemoryId::new(format!("memory-{}", uuid::Uuid::new_v4()));
        let compacted = MemoryRecord {
            memory_id: memory_id.clone(),
            memory_type: request.result_type,
            status: MemoryStatus::Active,
            body,
            confidence: sources.iter().map(|r| r.confidence).fold(0.0f32, f32::max),
            salience: sources.iter().map(|r| r.salience).fold(0.0f32, f32::max),
            scope: request.scope,
            history: Vec::new(),
            visibility: Visibility::Internal,
            title: request.title,
            links: Vec::new(),
            decay: None,
            embedding_refs: Vec::new(),
            superseded_by: None,
            contradicts: None,
        };
        let result = record_to_result(&compacted);
        records.insert(memory_id.0.clone(), compacted);
        if request.archive_sources {
            for id in &request.memory_ids {
                if let Some(record) = records.get_mut(&id.0) {
                    record.status = MemoryStatus::Archived;
                }
            }
        }
        Ok(result)
    }
}
