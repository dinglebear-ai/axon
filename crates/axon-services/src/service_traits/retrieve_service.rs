//! `RetrieveService` — fetch stored document chunks for a URL.
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §RetrieveService. The contract's `RetrievalRequest` DTO does not exist in
//! `axon-api`, and `crate::query::retrieve` module's exact request shape was
//! not verified against it in this pass — recorded as SKIP per the approved
//! wiring plan. `RetrieveResult` (the result side) already exists in
//! `axon-api::result` and is used as-is.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::result::RetrieveResult;

use crate::context::ServiceContext;
use crate::service_traits::not_implemented;

/// Minimal request shape for retrieval by URL. Mirrors the CLI/MCP `retrieve
/// <url>` surface; the full contract `RetrievalRequest` DTO (with variant
/// selection, token budgets, etc.) is deferred — see the module doc comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrieveRequest {
    pub url: String,
}

#[async_trait]
pub trait RetrieveService: Send + Sync {
    async fn retrieve(&self, request: RetrieveRequest) -> anyhow::Result<RetrieveResult>;
}

pub struct RetrieveServiceImpl {
    #[allow(dead_code)]
    ctx: Arc<ServiceContext>,
}

impl RetrieveServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl RetrieveService for RetrieveServiceImpl {
    async fn retrieve(&self, _request: RetrieveRequest) -> anyhow::Result<RetrieveResult> {
        Err(not_implemented("RetrieveService::retrieve"))
    }
}

/// Deterministic in-memory fake covering `RetrieveService::retrieve`.
#[derive(Default)]
pub struct FakeRetrieveService {
    documents: Mutex<std::collections::HashMap<String, String>>,
}

impl FakeRetrieveService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed(&self, url: impl Into<String>, content: impl Into<String>) {
        self.documents
            .lock()
            .unwrap()
            .insert(url.into(), content.into());
    }
}

#[async_trait]
impl RetrieveService for FakeRetrieveService {
    async fn retrieve(&self, request: RetrieveRequest) -> anyhow::Result<RetrieveResult> {
        let documents = self.documents.lock().unwrap();
        let Some(content) = documents.get(&request.url) else {
            return Ok(RetrieveResult {
                chunk_count: 0,
                content: String::new(),
                requested_url: Some(request.url),
                matched_url: None,
                truncated: false,
                warnings: vec!["no document indexed for this URL".to_string()],
                variant_errors: Vec::new(),
                token_estimate: None,
                next_cursor: None,
                remaining_tokens_estimate: None,
                backend: None,
                refresh_status: None,
            });
        };
        Ok(RetrieveResult {
            chunk_count: 1,
            content: content.clone(),
            requested_url: Some(request.url.clone()),
            matched_url: Some(request.url),
            truncated: false,
            warnings: Vec::new(),
            variant_errors: Vec::new(),
            token_estimate: None,
            next_cursor: None,
            remaining_tokens_estimate: None,
            backend: None,
            refresh_status: None,
        })
    }
}

#[cfg(test)]
#[path = "retrieve_service_tests.rs"]
mod tests;
