//! `MemoryService` — durable agent memory (remember/get/search/context/link/
//! forget).
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §MemoryService. **Finding vs. the approved wiring plan:** the plan assumed
//! `crate::memory`'s free functions (`remember`/`search`/`context`/`link`/
//! `forget`) take `axon_api::source::MemoryRequest`, but they actually take
//! `axon_api::mcp_schema::MemoryRequest` — a completely different,
//! subaction-flavored flat DTO (`MemoryRequest { subaction, id, body, query,
//! project, repo, file, ... }`) used by the CLI/MCP `memory` dispatch surface,
//! not the contract's typed request family (`MemorySearchRequest`,
//! `MemoryContextRequest`, `MemoryLinkRequest`, ...). Building a faithful
//! adapter between the two families (plus `MemoryItem`/`MemoryContext`
//! (crate-local, `crate::memory::mapping`) vs. `MemoryRecord`/
//! `MemorySearchResult`/`MemoryContextResult` (contract, `axon-api::source`))
//! is real orchestration work, not a thin wrap — so every production method
//! here is a stub; only the `Fake` implements real in-memory semantics using
//! the contract's own DTOs.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::source::{
    MemoryContextRequest, MemoryContextResult, MemoryId, MemoryLinkRequest, MemoryRecord,
    MemoryRequest, MemoryResult, MemorySearchRequest, MemorySearchResult,
};

use crate::context::ServiceContext;
use crate::service_traits::not_implemented;

#[async_trait]
pub trait MemoryService: Send + Sync {
    async fn remember(&self, request: MemoryRequest) -> anyhow::Result<MemoryResult>;
    async fn get(&self, memory_id: MemoryId) -> anyhow::Result<MemoryRecord>;
    async fn search(&self, request: MemorySearchRequest) -> anyhow::Result<MemorySearchResult>;
    async fn context(&self, request: MemoryContextRequest) -> anyhow::Result<MemoryContextResult>;
    async fn link(&self, request: MemoryLinkRequest) -> anyhow::Result<MemoryResult>;
    async fn forget(&self, memory_id: MemoryId) -> anyhow::Result<MemoryResult>;
}

pub struct MemoryServiceImpl {
    #[allow(dead_code)]
    ctx: Arc<ServiceContext>,
}

impl MemoryServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl MemoryService for MemoryServiceImpl {
    async fn remember(&self, _request: MemoryRequest) -> anyhow::Result<MemoryResult> {
        Err(not_implemented("MemoryService::remember"))
    }

    async fn get(&self, _memory_id: MemoryId) -> anyhow::Result<MemoryRecord> {
        Err(not_implemented("MemoryService::get"))
    }

    async fn search(&self, _request: MemorySearchRequest) -> anyhow::Result<MemorySearchResult> {
        Err(not_implemented("MemoryService::search"))
    }

    async fn context(&self, _request: MemoryContextRequest) -> anyhow::Result<MemoryContextResult> {
        Err(not_implemented("MemoryService::context"))
    }

    async fn link(&self, _request: MemoryLinkRequest) -> anyhow::Result<MemoryResult> {
        Err(not_implemented("MemoryService::link"))
    }

    async fn forget(&self, _memory_id: MemoryId) -> anyhow::Result<MemoryResult> {
        Err(not_implemented("MemoryService::forget"))
    }
}

fn fake_record(memory_id: MemoryId, request: &MemoryRequest) -> MemoryRecord {
    MemoryRecord {
        memory_id,
        memory_type: request.memory_type,
        status: axon_api::source::MemoryStatus::Active,
        body: request.body.clone(),
        confidence: request.confidence,
        salience: request.salience,
        scope: request.scope.clone(),
        history: Vec::new(),
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

    async fn forget(&self, memory_id: MemoryId) -> anyhow::Result<MemoryResult> {
        let mut records = self.records.lock().unwrap();
        let record = records
            .get_mut(&memory_id.0)
            .ok_or_else(|| anyhow::anyhow!("memory {} not found", memory_id.0))?;
        record.status = axon_api::source::MemoryStatus::Forgotten;
        Ok(record_to_result(record))
    }
}

#[cfg(test)]
#[path = "memory_service_tests.rs"]
mod tests;
