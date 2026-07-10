//! `DocumentService` — document/chunk read surface (list/get/chunks/chunk).
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §DocumentService. All contract DTOs (`DocumentListRequest`,
//! `DocumentSummary`, `DocumentDetail`, `ChunkListRequest`, `ChunkGetRequest`,
//! `ChunkSummary`, `ChunkDetail`) already exist in `axon-api::source::listing`
//! — but no free function in `axon-services` lists/gets documents or chunks
//! by these DTOs today (`document.rs` only has pagination/cursor helpers and
//! a single-URL stored-source reader keyed by URL string, not `DocumentId`).
//! All four methods are therefore FAKE_ONLY; only the `Fake` implements real
//! (in-memory) semantics.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::source::{
    ChunkDetail, ChunkGetRequest, ChunkListRequest, ChunkSummary, DocumentDetail, DocumentId,
    DocumentListRequest, DocumentSummary, Page,
};

use crate::context::ServiceContext;
use crate::service_traits::not_implemented;

#[async_trait]
pub trait DocumentService: Send + Sync {
    async fn list(&self, request: DocumentListRequest) -> anyhow::Result<Page<DocumentSummary>>;
    async fn get(&self, document_id: DocumentId) -> anyhow::Result<DocumentDetail>;
    async fn chunks(&self, request: ChunkListRequest) -> anyhow::Result<Page<ChunkSummary>>;
    async fn chunk(&self, request: ChunkGetRequest) -> anyhow::Result<ChunkDetail>;
}

pub struct DocumentServiceImpl {
    #[allow(dead_code)]
    ctx: Arc<ServiceContext>,
}

impl DocumentServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl DocumentService for DocumentServiceImpl {
    async fn list(&self, _request: DocumentListRequest) -> anyhow::Result<Page<DocumentSummary>> {
        Err(not_implemented("DocumentService::list"))
    }

    async fn get(&self, _document_id: DocumentId) -> anyhow::Result<DocumentDetail> {
        Err(not_implemented("DocumentService::get"))
    }

    async fn chunks(&self, _request: ChunkListRequest) -> anyhow::Result<Page<ChunkSummary>> {
        Err(not_implemented("DocumentService::chunks"))
    }

    async fn chunk(&self, _request: ChunkGetRequest) -> anyhow::Result<ChunkDetail> {
        Err(not_implemented("DocumentService::chunk"))
    }
}

/// Deterministic in-memory fake covering every `DocumentService` method.
#[derive(Default)]
pub struct FakeDocumentService {
    documents: Mutex<std::collections::HashMap<String, DocumentDetail>>,
}

impl FakeDocumentService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed(&self, detail: DocumentDetail) {
        self.documents
            .lock()
            .unwrap()
            .insert(detail.document_id.0.clone(), detail);
    }
}

#[async_trait]
impl DocumentService for FakeDocumentService {
    async fn list(&self, request: DocumentListRequest) -> anyhow::Result<Page<DocumentSummary>> {
        let documents = self.documents.lock().unwrap();
        let limit = request.limit.unwrap_or(50);
        let items = documents
            .values()
            .take(limit as usize)
            .map(|d| DocumentSummary {
                document_id: d.document_id.clone(),
                source_id: d.source_id.clone(),
                source_item_key: d.source_item_key.clone(),
                status: d.status,
                chunk_count: d.chunk_count,
                vector_point_count: d.vector_point_count,
                content_kind: d.content_kind,
                title: d.title.clone(),
                path: d.path.clone(),
                graph_refs: d.graph_refs.clone(),
            })
            .collect();
        Ok(Page {
            items,
            next_cursor: None,
            limit,
            total: Some(documents.len() as u64),
        })
    }

    async fn get(&self, document_id: DocumentId) -> anyhow::Result<DocumentDetail> {
        self.documents
            .lock()
            .unwrap()
            .get(&document_id.0)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("document {} not found", document_id.0))
    }

    async fn chunks(&self, request: ChunkListRequest) -> anyhow::Result<Page<ChunkSummary>> {
        let documents = self.documents.lock().unwrap();
        let chunks = documents
            .get(&request.document_id.0)
            .map(|d| d.chunks.clone())
            .unwrap_or_default();
        let limit = request.limit.unwrap_or(50);
        Ok(Page {
            items: chunks.into_iter().take(limit as usize).collect(),
            next_cursor: None,
            limit,
            total: None,
        })
    }

    async fn chunk(&self, request: ChunkGetRequest) -> anyhow::Result<ChunkDetail> {
        let documents = self.documents.lock().unwrap();
        let document = documents
            .get(&request.document_id.0)
            .ok_or_else(|| anyhow::anyhow!("document {} not found", request.document_id.0))?;
        let summary = document
            .chunks
            .iter()
            .find(|c| c.chunk_id == request.chunk_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("chunk {} not found", request.chunk_id.0))?;
        Ok(ChunkDetail {
            chunk_id: summary.chunk_id,
            document_id: summary.document_id,
            chunk_index: summary.chunk_index,
            chunk_locator: summary.chunk_locator,
            source_range: summary.source_range,
            metadata: summary.metadata,
            graph_refs: summary.graph_refs,
            vector_refs: summary.vector_refs,
            content_hash: String::new(),
            content: None,
            payload: axon_api::source::MetadataMap::new(),
            embedding_metadata: axon_api::source::MetadataMap::new(),
        })
    }
}

#[cfg(test)]
#[path = "document_service_tests.rs"]
mod tests;
