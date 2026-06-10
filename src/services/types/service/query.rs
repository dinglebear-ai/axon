// ── Query / retrieve / ask / evaluate / suggest ──────────────────────────────

// ask_explain types moved to crate::core::ask_explain (A-C1 cycle break)

pub use crate::core::ask_explain::*;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryHit {
    pub rank: u64,
    pub score: f64,
    pub rerank_score: f64,
    pub url: String,
    pub source: String,
    pub snippet: String,
    pub chunk_index: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunking_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_extraction_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryResult {
    pub results: Vec<QueryHit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentBackend {
    /// Content reconstructed from Qdrant vector chunks.
    Qdrant,
    /// Content read from a stored source file (markdown/html).
    StoredSource,
    /// Content fetched from a live scrape refresh.
    LiveScrape,
}

impl std::fmt::Display for DocumentBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Qdrant => write!(f, "qdrant"),
            Self::StoredSource => write!(f, "stored_source"),
            Self::LiveScrape => write!(f, "live_scrape"),
        }
    }
}

/// A slice of document content with windowing/pagination metadata.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PagedDocument {
    /// The document content (markdown).
    pub content: String,
    /// True if the document was truncated to fit the response budget.
    pub truncated: bool,
    /// Conservative estimate of tokens in this slice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_estimate: Option<usize>,
    /// Opaque cursor for fetching the next slice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Conservative estimate of remaining tokens after this slice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_tokens_estimate: Option<usize>,
    /// The backend that provided this content.
    pub backend: DocumentBackend,
}

impl PagedDocument {
    /// Default token budget for inline document reads (10k tokens).
    pub const DEFAULT_TOKEN_BUDGET: usize = 10_000;
    /// Conservative approximation: 1 token ≈ 4 characters.
    pub const CHARS_PER_TOKEN: usize = 4;

    /// Window a full document into a paged slice based on a token budget and cursor.
    pub fn from_full_content(
        full_content: &str,
        cursor: Option<&str>,
        token_budget: Option<usize>,
        backend: DocumentBackend,
    ) -> Self {
        let budget = token_budget.unwrap_or(Self::DEFAULT_TOKEN_BUDGET).max(1);
        let char_budget = budget.saturating_mul(Self::CHARS_PER_TOKEN);

        let mut start_offset = cursor.and_then(|c| c.parse::<usize>().ok()).unwrap_or(0);

        if start_offset >= full_content.len() {
            return Self {
                content: String::new(),
                truncated: false,
                token_estimate: Some(0),
                next_cursor: None,
                remaining_tokens_estimate: Some(0),
                backend,
            };
        }
        while start_offset < full_content.len() && !full_content.is_char_boundary(start_offset) {
            start_offset += 1;
        }
        if start_offset >= full_content.len() {
            return Self {
                content: String::new(),
                truncated: false,
                token_estimate: Some(0),
                next_cursor: None,
                remaining_tokens_estimate: Some(0),
                backend,
            };
        }

        // Slice at char boundaries to avoid panics.
        // find_at_char_boundary(full_content, start_offset + char_budget)
        let end_limit = start_offset
            .saturating_add(char_budget)
            .min(full_content.len());
        let actual_end = if full_content.is_char_boundary(end_limit) {
            end_limit
        } else {
            // Step back to previous boundary
            let mut pos = end_limit - 1;
            while pos > start_offset && !full_content.is_char_boundary(pos) {
                pos -= 1;
            }
            pos
        };

        let content = full_content[start_offset..actual_end].to_string();
        let truncated = actual_end < full_content.len();

        let token_estimate = content.len().div_ceil(Self::CHARS_PER_TOKEN);
        let remaining_chars = full_content.len() - actual_end;
        let remaining_tokens_estimate = remaining_chars.div_ceil(Self::CHARS_PER_TOKEN);

        let next_cursor = if truncated {
            Some(actual_end.to_string())
        } else {
            None
        };

        Self {
            content,
            truncated,
            token_estimate: Some(token_estimate),
            next_cursor,
            remaining_tokens_estimate: Some(remaining_tokens_estimate),
            backend,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RetrieveResult {
    pub chunk_count: usize,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_url: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variant_errors: Vec<ServiceRetrieveVariantError>,

    // Document windowing fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_estimate: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_tokens_estimate: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<DocumentBackend>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServiceRetrieveVariantError {
    pub url: String,
    pub error: String,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskDiagnostics {
    pub candidate_pool: usize,
    pub reranked_pool: usize,
    pub chunks_selected: usize,
    pub full_docs_selected: usize,
    pub supplemental_selected: usize,
    pub context_chars: usize,
    #[serde(default)]
    pub full_doc_fetch_skipped: bool,
    #[serde(default)]
    pub full_doc_fetch_skip_reason: String,
    #[serde(default)]
    pub detected_complexity: String,
    #[serde(default)]
    pub resolved_full_docs: usize,
    #[serde(default)]
    pub full_docs_source: String,
    pub min_relevance_score: f64,
    #[serde(default)]
    pub ask_candidate_limit: usize,
    #[serde(default)]
    pub ask_chunk_limit: usize,
    #[serde(default)]
    pub ask_backfill_chunks: usize,
    #[serde(default)]
    pub ask_doc_chunk_limit: usize,
    #[serde(default)]
    pub ask_hybrid_candidates: usize,
    #[serde(default)]
    pub ask_full_docs_configured: usize,
    #[serde(default)]
    pub ask_full_docs_explicit: bool,
    #[serde(default)]
    pub ask_fulldoc_skip_enabled: bool,
    #[serde(default)]
    pub ask_max_context_chars: usize,
    pub doc_fetch_concurrency: usize,
    pub top_domains: Vec<String>,
    pub authority_ratio: f64,
    #[serde(default)]
    pub configured_authority_ratio: f64,
    #[serde(default)]
    pub product_authority_ratio: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corpus_health: Option<CorpusHealthDiagnostic>,
}

// CorpusHealthKind and CorpusHealthDiagnostic moved to crate::core::ask_explain
// (A-C1 cycle break); re-exported above via `pub use crate::core::ask_explain::*`.

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AskTiming {
    pub retrieval: u128,
    pub context_build: u128,
    pub llm: u128,
    pub total: u128,
    // Populated only when ask_diagnostics=true; otherwise omitted via skip_serializing_if.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tei_embed_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qdrant_primary_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qdrant_secondary_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_select_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_doc_fetch_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supplemental_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_ttft_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_total_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub streamed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalize_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskResult {
    pub query: String,
    pub answer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub diagnostics: Option<AskDiagnostics>,
    #[serde(default)]
    pub explain: Option<AskExplainTrace>,
    pub timing_ms: AskTiming,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EvaluateDiagnostics {
    pub candidate_pool: usize,
    pub reranked_pool: usize,
    pub chunks_selected: usize,
    pub full_docs_selected: usize,
    pub supplemental_selected: usize,
    pub context_chars: usize,
    pub min_relevance_score: f64,
    pub doc_fetch_concurrency: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvaluateTiming {
    pub retrieval: u128,
    pub context_build: u128,
    pub rag_llm: u128,
    pub baseline_llm: u128,
    pub research_elapsed_ms: u128,
    pub analysis_llm_ms: u128,
    pub total: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvaluateCrawlEnqueueOutcome {
    pub url: String,
    pub job_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EvaluateResult {
    pub query: String,
    pub rag_answer: String,
    pub baseline_answer: String,
    pub analysis_answer: String,
    pub source_urls: Vec<String>,
    pub crawl_suggestions: Vec<Suggestion>,
    pub crawl_enqueue_outcomes: Vec<EvaluateCrawlEnqueueOutcome>,
    pub ref_chunk_count: usize,
    pub diagnostics: Option<EvaluateDiagnostics>,
    pub timing_ms: EvaluateTiming,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Suggestion {
    pub url: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SuggestResult {
    pub suggestions: Vec<Suggestion>,
}
