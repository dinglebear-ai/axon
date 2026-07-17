//! `AskService` — RAG ask/evaluate/suggest and direct LLM chat.
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §AskService. `ask` mirrors the MCP `handle_ask` handler exactly: every
//! `AskRequest` override field is threaded through
//! `crate::transport::apply_ask_overrides` (the same helper the MCP/REST
//! transports use) before calling `crate::query::ask`, so no override is
//! silently dropped. `suggest` wraps `crate::query::suggest` and now also
//! applies `request.limit`/`request.collection` onto a cloned `Config`
//! before calling it, mirroring the transport handler. `evaluate` wraps
//! `crate::query::evaluate` and takes a minimal `EvaluationRequest{ question
//! }` DTO — the contract's `EvaluationRequest` wrapper doesn't exist in
//! `axon-api` and would only ever carry that one field today, so it is
//! defined locally here rather than skipped (fits the <=30-line DTO rule).
//! `chat` calls the configured chat-purpose LLM backend directly. Conversation
//! persistence is intentionally outside this request/response operation.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::mcp_schema::AskRequest;
use axon_api::mcp_schema::SuggestRequest;
use axon_api::result::{AskResult, AskTiming, EvaluateResult, EvaluateTiming, SuggestResult};

use crate::context::ServiceContext;
use crate::transport::{AskTransportOverrides, apply_ask_overrides};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatRequest {
    pub session_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatResult {
    pub session_id: String,
    pub reply: String,
    pub model: Option<String>,
}

/// Minimal local DTO for `AskService::evaluate` — the contract's
/// `EvaluationRequest` has no `axon-api` analog and would only ever carry
/// this one field today. See the module doc comment for the DTO-rule
/// rationale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationRequest {
    pub question: String,
}

#[async_trait]
pub trait AskService: Send + Sync {
    async fn ask(&self, request: AskRequest) -> anyhow::Result<AskResult>;
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResult>;
    async fn evaluate(&self, request: EvaluationRequest) -> anyhow::Result<EvaluateResult>;
    async fn suggest(&self, request: SuggestRequest) -> anyhow::Result<SuggestResult>;
}

pub struct AskServiceImpl {
    ctx: Arc<ServiceContext>,
}

impl AskServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl AskService for AskServiceImpl {
    async fn ask(&self, request: AskRequest) -> anyhow::Result<AskResult> {
        let question = request.query.clone().unwrap_or_default();
        let cfg = apply_ask_overrides(
            self.ctx.cfg(),
            AskTransportOverrides {
                collection: request.collection,
                since: request.since,
                before: request.before,
                diagnostics: request.diagnostics,
                explain: request.explain,
                hybrid_search: request.hybrid_search,
                ask_chunk_limit: request.ask_chunk_limit,
                ask_full_docs: request.ask_full_docs,
                ask_max_context_chars: request.ask_max_context_chars,
                ask_hybrid_candidates: request.ask_hybrid_candidates,
                ask_min_relevance_score: request.ask_min_relevance_score,
                ask_doc_chunk_limit: request.ask_doc_chunk_limit,
                ask_doc_fetch_concurrency: request.ask_doc_fetch_concurrency,
                ask_backfill_chunks: request.ask_backfill_chunks,
                ask_candidate_limit: request.ask_candidate_limit,
                ask_min_citations_nontrivial: request.ask_min_citations_nontrivial,
                ask_authoritative_domains: request.ask_authoritative_domains,
                ask_authoritative_boost: request.ask_authoritative_boost,
            },
        );
        crate::query::ask(&self.ctx, &cfg, &question, None)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResult> {
        let message = request.message.trim();
        if message.is_empty() {
            anyhow::bail!("chat message is required");
        }
        let completion_request = axon_llm::CompletionRequest::new(message)
            .backend_from_config_for(self.ctx.cfg(), axon_llm::LlmModelPurpose::Chat)
            .stream(false);
        let model = completion_request.model.clone();
        let completion = axon_llm::complete_text(completion_request)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        Ok(ChatResult {
            session_id: request
                .session_id
                .unwrap_or_else(|| format!("chat_{}", uuid::Uuid::new_v4().simple())),
            reply: completion.text,
            model,
        })
    }

    async fn evaluate(&self, request: EvaluationRequest) -> anyhow::Result<EvaluateResult> {
        crate::query::evaluate(&self.ctx, self.ctx.cfg(), &request.question)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn suggest(&self, request: SuggestRequest) -> anyhow::Result<SuggestResult> {
        let mut cfg = self.ctx.cfg().clone();
        if let Some(collection) = request.collection {
            cfg.collection = collection;
        }
        if let Some(limit) = request.limit {
            cfg.search_limit = limit.clamp(1, 100);
        }
        crate::query::suggest(&cfg, request.focus.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

/// Deterministic in-memory fake covering every `AskService` method.
#[derive(Default)]
pub struct FakeAskService {
    answers: Mutex<std::collections::HashMap<String, String>>,
}

impl FakeAskService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed_answer(&self, question: impl Into<String>, answer: impl Into<String>) {
        self.answers
            .lock()
            .unwrap()
            .insert(question.into(), answer.into());
    }
}

#[async_trait]
impl AskService for FakeAskService {
    async fn ask(&self, request: AskRequest) -> anyhow::Result<AskResult> {
        let query = request.query.unwrap_or_default();
        let answer = self
            .answers
            .lock()
            .unwrap()
            .get(&query)
            .cloned()
            .unwrap_or_else(|| "fake answer".to_string());
        Ok(AskResult {
            query,
            answer,
            citations: Vec::new(),
            citation_validation: None,
            session: None,
            warnings: Vec::new(),
            diagnostics: None,
            explain: None,
            timing_ms: AskTiming {
                retrieval: 0,
                context_build: 0,
                llm: 0,
                total: 0,
                tei_embed_ms: None,
                qdrant_primary_ms: None,
                qdrant_secondary_ms: None,
                rerank_ms: None,
                top_select_ms: None,
                full_doc_fetch_ms: None,
                supplemental_ms: None,
                llm_ttft_ms: None,
                llm_total_ms: None,
                streamed: None,
                normalize_ms: None,
            },
        })
    }

    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResult> {
        Ok(ChatResult {
            session_id: request
                .session_id
                .unwrap_or_else(|| "fake-session".to_string()),
            reply: format!("fake reply to: {}", request.message),
            model: Some("fake-chat-model".to_string()),
        })
    }

    async fn evaluate(&self, request: EvaluationRequest) -> anyhow::Result<EvaluateResult> {
        Ok(EvaluateResult {
            query: request.question,
            rag_answer: "fake rag answer".to_string(),
            baseline_answer: "fake baseline answer".to_string(),
            analysis_answer: "fake analysis".to_string(),
            citations: Vec::new(),
            source_urls: Vec::new(),
            crawl_suggestions: Vec::new(),
            crawl_enqueue_outcomes: Vec::new(),
            ref_chunk_count: 0,
            diagnostics: None,
            timing_ms: EvaluateTiming {
                retrieval: 0,
                context_build: 0,
                rag_llm: 0,
                baseline_llm: 0,
                research_elapsed_ms: 0,
                analysis_llm_ms: 0,
                total: 0,
            },
        })
    }

    async fn suggest(&self, _request: SuggestRequest) -> anyhow::Result<SuggestResult> {
        Ok(SuggestResult {
            suggestions: Vec::new(),
        })
    }
}

#[cfg(test)]
#[path = "ask_service_tests.rs"]
mod tests;
