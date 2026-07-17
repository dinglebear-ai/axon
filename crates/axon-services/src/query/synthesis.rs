//! `ask`/`evaluate` LLM synthesis, ported off legacy `axon_vector::ops::commands::ask`
//! (issue #298 cutover — the read + synthesis half of `ask`/`evaluate`/`suggest`/
//! `retrieve` moving from `axon-vector` onto `axon-retrieval` + `axon-vectors`).
//!
//! This module owns the SYNTHESIS half of `ask`: given an already-built
//! [`AskContext`] (produced by [`super::ask_retrieval::retrieval_ask_context`]
//! through the `axon-retrieval` engine), it builds the synthesis prompt, calls
//! the configured LLM backend, validates + repairs citations, and assembles the
//! typed [`crate::types::AskResult`]. Final LLM synthesis is deliberately kept
//! OUT of `axon-retrieval` (see that crate's `CLAUDE.md` boundary) — this is its
//! home instead.
//!
//! `AskContext` here is a **new, narrower type** distinct from legacy
//! `axon_vector::ops::commands::ask::AskContext`: the legacy type also carried
//! an `explain: Option<AskExplainTrace>` field, populated only by the legacy
//! reranker's own `build_ask_context` when `cfg.ask_explain` was set. This
//! `AskContext` has no `explain` field at all — every value flowing through
//! this module is built via `from_retrieval`. `ask --explain`'s trace is now
//! built separately, straight from the retrieval hits, by
//! `super::ask_retrieval::explain::build_explain_trace` and attached in
//! [`assemble::assemble_explain_result`] instead of carried on `AskContext`.

use axon_core::ask_explain::{
    AskExplainFullDocFetchError, CorpusHealthDiagnostic, CorpusHealthKind,
};

pub(crate) mod assemble;
pub(crate) mod completion;
pub(crate) mod normalize;
pub(crate) mod output;
pub(crate) mod pipeline;
pub(crate) mod prompt;
pub(crate) mod timing;

pub(crate) use pipeline::validate_ask_llm_config;
pub use pipeline::{ask_result_from_context, ask_result_from_context_with_deltas};

/// Synthesis-ready ask context: the retrieved/rendered context string plus the
/// bookkeeping [`assemble::assemble_ask_result`] needs to fill in
/// `AskDiagnostics`/timing. Ports the non-explain fields of legacy
/// `axon_vector::ops::commands::ask::context::AskContext`.
pub struct AskContext {
    pub context: String,
    pub candidate_count: usize,
    pub reranked_count: usize,
    pub chunks_selected: usize,
    pub full_docs_selected: usize,
    pub supplemental_count: usize,
    pub retrieval_elapsed_ms: u128,
    pub context_elapsed_ms: u128,
    pub diagnostic_sources: Vec<String>,
    /// Canonical lineage for the chunks admitted to the synthesis context.
    pub citations: Vec<axon_api::CanonicalCitation>,
    pub top_domains: Vec<String>,
    pub authoritative_ratio: f64,
    pub configured_authority_ratio: f64,
    pub product_authority_ratio: f64,
    pub corpus_health: CorpusHealthDiagnostic,
    /// True when full-doc fetch was skipped or never attempted. Always `true`
    /// on the retrieval-engine path — full-doc/supplemental staging is a
    /// legacy-reranker-only concept.
    pub full_doc_fetch_skipped: bool,
    /// Static reason string ("retrieval_engine" for every value built here).
    pub full_doc_fetch_skip_reason: &'static str,
    pub full_doc_fetch_errors: Vec<AskExplainFullDocFetchError>,
    /// Coarse query-complexity signal. Always `"simple"` on the
    /// retrieval-engine path — the adaptive complexity classifier is part of
    /// the legacy reranker's query-rewrite stage, not reproduced here.
    pub detected_complexity: &'static str,
    pub resolved_full_docs: usize,
    pub full_docs_source: &'static str,
    pub warnings: Vec<String>,
}

impl AskContext {
    /// Build an [`AskContext`] from a context string produced by the
    /// `axon-retrieval` engine.
    ///
    /// The caller (`ask_retrieval::retrieval_ask_context`) runs hybrid
    /// retrieval through `axon_retrieval::run_query`, formats the returned
    /// hits into the `Sources:\n ## Top Chunk [S#]: …` context string the
    /// synthesis prompt expects, and passes it here along with retrieval
    /// bookkeeping. Full-doc/supplemental/rerank stages are not run on this
    /// path, so their counts are zero and the fetch-skip reason is
    /// `"retrieval_engine"`.
    pub fn from_retrieval(
        context: String,
        candidate_count: usize,
        chunks_selected: usize,
        retrieval_elapsed_ms: u128,
        top_domains: Vec<String>,
        selected_urls: &[String],
        warnings: Vec<String>,
    ) -> AskContext {
        let corpus_health =
            classify_corpus_health(&top_domains, selected_urls, candidate_count, context.len());
        AskContext {
            context,
            candidate_count,
            reranked_count: candidate_count,
            chunks_selected,
            full_docs_selected: 0,
            supplemental_count: 0,
            retrieval_elapsed_ms,
            context_elapsed_ms: 0,
            diagnostic_sources: selected_urls.to_vec(),
            citations: Vec::new(),
            top_domains,
            authoritative_ratio: 0.0,
            configured_authority_ratio: 0.0,
            product_authority_ratio: 0.0,
            corpus_health,
            full_doc_fetch_skipped: true,
            full_doc_fetch_skip_reason: "retrieval_engine",
            full_doc_fetch_errors: Vec::new(),
            detected_complexity: "simple",
            resolved_full_docs: 0,
            full_docs_source: "retrieval_engine",
            warnings,
        }
    }
}

/// Classify overall corpus health for `ask`/`evaluate` diagnostics. Ports
/// legacy `axon_vector`'s `classify_corpus_health` verbatim.
pub(crate) fn classify_corpus_health(
    top_domains: &[String],
    selected_urls: &[String],
    candidate_pool: usize,
    context_chars: usize,
) -> CorpusHealthDiagnostic {
    let top_domain_count = top_domains.len();
    let selected_domain_count = selected_urls
        .iter()
        .filter_map(|url| reqwest::Url::parse(url).ok())
        .filter_map(|url| url.host_str().map(str::to_string))
        .collect::<std::collections::HashSet<_>>()
        .len();

    let (kind, reason) = if candidate_pool == 0 {
        (
            CorpusHealthKind::NoRetrievalCandidates,
            "retrieval returned no candidates".to_string(),
        )
    } else if selected_urls.is_empty() {
        (
            CorpusHealthKind::RetrievedNotSelected,
            "retrieval returned candidates but none reached selected context".to_string(),
        )
    } else if context_chars < 2_000 {
        (
            CorpusHealthKind::ThinDomain,
            "selected context is very small; indexed coverage may be thin".to_string(),
        )
    } else if top_domain_count == 0 {
        (
            CorpusHealthKind::Unknown,
            "top-domain diagnostics were unavailable".to_string(),
        )
    } else {
        (
            CorpusHealthKind::Healthy,
            "retrieval produced selected context".to_string(),
        )
    };

    CorpusHealthDiagnostic {
        kind,
        reason,
        selected_domain_count,
        top_domain_count,
    }
}
