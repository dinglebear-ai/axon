//! `ask` retrieval half routed through the new `axon-retrieval` engine.
//!
//! Issue #298 cutover: the SEARCH + CONTEXT portion of `ask` embeds +
//! hybrid-searches through [`axon_retrieval::run_query`] (dense + bm42 RRF)
//! instead of legacy axon-vector's `build_ask_context` reranker + full-doc
//! fetcher. The retrieved chunks are formatted into the `Sources:\n ## Top
//! Chunk [S#]: …` context string the synthesis prompt expects, wrapped in
//! [`super::synthesis::AskContext`], and handed to the synthesis pipeline in
//! `super::synthesis` (also ported off legacy axon-vector in this same
//! cutover — see that module's doc comment), which runs the LLM completion,
//! citation validation/repair, and result assembly.
//!
//! `cfg.ask_explain` (`ask --explain`, used by `train`) is the #298 finale:
//! the legacy reranker (and the `axon-vector` crate that housed it) is
//! retired entirely, so explain requests now run the SAME retrieval pass as
//! a normal `ask` and skip only the LLM call — see [`explain`] for how the
//! trace is derived from the retrieval engine's hits, and
//! [`super::synthesis::assemble::assemble_explain_result`] for the
//! no-synthesis result assembly.

use std::collections::BTreeSet;
use std::error::Error;

use axon_api::source::{AuthSnapshot, OperationKind};
use axon_core::config::Config;
use axon_core::error::ServiceError;
use axon_core::logging::log_info;
use axon_retrieval::{QueryServiceHit, QueryServiceRequest, run_query};

use super::provider_execution::ReadExecution;
use super::synthesis::assemble::assemble_explain_result;
use super::synthesis::normalize;
use super::synthesis::{AskContext, ask_result_from_context, ask_result_from_context_with_deltas};
use crate::context::ServiceContext;
use crate::types::AskResult;

mod explain;

/// Prefix that opens every ask context blob; the synthesis prompt keys off it.
const CONTEXT_PREFIX: &str = "Sources:\n";
/// Separator between context entries (matches the legacy builder byte-for-byte).
const CONTEXT_SEPARATOR: &str = "\n\n---\n\n";

/// Run the ask retrieval half through `axon-retrieval` and synthesize the
/// answer with the existing LLM pipeline.
///
/// When `on_delta` is `Some`, synthesis streams token deltas through it.
/// Errors clearly when no read-plane runtime/config is available — never falls
/// back to the legacy vector retrieval path.
pub async fn ask_via_retrieval<F>(
    ctx: &ServiceContext,
    cfg: &Config,
    question: &str,
    on_delta: Option<F>,
) -> Result<AskResult, Box<dyn Error>>
where
    F: FnMut(&str) + Send,
{
    ask_via_retrieval_with_auth(ctx, cfg, question, on_delta, None).await
}

/// Auth-aware ask path used by authenticated transports.
pub async fn ask_via_retrieval_with_auth<F>(
    ctx: &ServiceContext,
    cfg: &Config,
    question: &str,
    on_delta: Option<F>,
    auth_snapshot: Option<AuthSnapshot>,
) -> Result<AskResult, Box<dyn Error>>
where
    F: FnMut(&str) + Send,
{
    if cfg.qdrant_url.trim().is_empty() || cfg.tei_url.trim().is_empty() {
        return Err(Box::new(ServiceError::new(
            "ask requires both QDRANT_URL and TEI_URL to be configured for the retrieval engine"
                .to_string(),
        )));
    }

    let ask_started = std::time::Instant::now();

    // Explain mode (`ask --explain`, used by `train`) runs the exact same
    // retrieval pass as a normal ask, then traces it into an
    // `AskExplainTrace` instead of calling the LLM. See `explain` for the
    // trace-building logic and its module doc for what narrowed relative to
    // the retired legacy reranker's trace.
    if cfg.ask_explain {
        let (ask_ctx, hits) = retrieval_ask_context_with_hits(
            ctx,
            cfg,
            question,
            "ask",
            false,
            auth_snapshot.clone(),
        )
        .await?;
        let trace = explain::build_explain_trace(
            cfg,
            question,
            &hits,
            ask_ctx.chunks_selected,
            &ask_ctx.context,
        );
        return Ok(assemble_explain_result(
            cfg,
            question,
            &ask_ctx,
            trace,
            ask_started.elapsed().as_millis(),
        ));
    }

    let ask_ctx = retrieval_ask_context_with_auth(ctx, cfg, question, "ask", auth_snapshot).await?;

    let synth = match on_delta {
        Some(cb) => {
            ask_result_from_context_with_deltas(cfg, question, ask_ctx, ask_started, cb).await
        }
        None => ask_result_from_context(cfg, question, ask_ctx, ask_started).await,
    };

    synth.map_err(|e| -> Box<dyn Error> {
        Box::new(ServiceError::new(format!(
            "ask synthesis failed for {}: {e}",
            question.chars().take(80).collect::<String>()
        )))
    })
}

/// Run the shared RAG-retrieval seam through `axon-retrieval` and format the
/// hits into an [`AskContext`] ready for synthesis.
///
/// This is the exact retrieval + context-build step used by both `ask` (issue
/// #298, PR #348) and `evaluate` (this slice): embed the question, hybrid-search
/// (dense + bm42 RRF) via [`run_query`], and render the returned chunks into the
/// `Sources:\n ## Top Chunk [S#]: …` context string. `label` disambiguates the
/// log marker (`"ask"` / `"evaluate"`). Errors clearly when no read-plane
/// runtime/config is available — never falls back to the legacy vector retrieval
/// path.
pub(crate) async fn retrieval_ask_context(
    ctx: &ServiceContext,
    cfg: &Config,
    question: &str,
    label: &str,
) -> Result<AskContext, Box<dyn Error>> {
    retrieval_ask_context_with_auth(ctx, cfg, question, label, None).await
}

pub(crate) async fn retrieval_ask_context_with_auth(
    ctx: &ServiceContext,
    cfg: &Config,
    question: &str,
    label: &str,
    auth_snapshot: Option<AuthSnapshot>,
) -> Result<AskContext, Box<dyn Error>> {
    let (ask_ctx, _hits) =
        retrieval_ask_context_with_hits(ctx, cfg, question, label, true, auth_snapshot).await?;
    Ok(ask_ctx)
}

/// Same as [`retrieval_ask_context`], but also returns the full candidate-pool
/// hits (in retrieval order) alongside the built [`AskContext`]. The plain
/// `ask`/`evaluate` callers only need the context, so [`retrieval_ask_context`]
/// discards the hits; `ask --explain` (via [`explain::build_explain_trace`])
/// needs both.
async fn retrieval_ask_context_with_hits(
    ctx: &ServiceContext,
    cfg: &Config,
    question: &str,
    label: &str,
    diversify_documents: bool,
    auth_snapshot: Option<AuthSnapshot>,
) -> Result<(AskContext, Vec<QueryServiceHit>), Box<dyn Error>> {
    if cfg.qdrant_url.trim().is_empty() || cfg.tei_url.trim().is_empty() {
        return Err(Box::new(ServiceError::new(format!(
            "{label} requires both QDRANT_URL and TEI_URL to be configured for the retrieval engine"
        ))));
    }

    let retrieval_started = std::time::Instant::now();
    let execution = ReadExecution::begin(
        ctx,
        cfg,
        OperationKind::Query,
        serde_json::json!({
            "query": question,
            "collection": cfg.collection,
            "label": label,
        }),
        auth_snapshot,
    )
    .await?;
    let store = execution.scheduled_vectors();
    let provider = execution.scheduled_embedding();
    let provider_id = execution.embedding_provider_id();
    let model = execution.embedding_model();
    let dimensions = execution.embedding_dimensions();

    // The ask/evaluate path fetches a wider candidate pool than plain `query`
    // before trimming to the context entries synthesis will read.
    // `ask_hybrid_candidates` (env `AXON_ASK_HYBRID_CANDIDATES`, default 150) is
    // the fetch width; `ask_chunk_limit` (default 24) bounds the entries
    // rendered into context.
    let fetch_limit = cfg.ask_hybrid_candidates.max(cfg.ask_chunk_limit).max(1) as u32;
    let (since, before) = super::retrieval::normalize_time_bounds(cfg, chrono::Utc::now())?;

    log_info(&format!(
        "{label} retrieval: axon-retrieval engine collection={} fetch_limit={} chunk_limit={}",
        cfg.collection, fetch_limit, cfg.ask_chunk_limit,
    ));

    let raw_result = run_query(
        store,
        provider,
        provider_id,
        model,
        dimensions,
        QueryServiceRequest {
            query: question.to_string(),
            collection: cfg.collection.clone(),
            limit: fetch_limit,
            hybrid: cfg.hybrid_search_enabled,
            since,
            before,
        },
    )
    .await;
    execution.finish(ctx, &raw_result).await;
    let result = raw_result.map_err(|e| -> Box<dyn Error> {
        Box::new(ServiceError::new(format!(
            "{label} retrieval failed for {}: {e}",
            question.chars().take(80).collect::<String>()
        )))
    })?;

    let retrieval_elapsed_ms = retrieval_started.elapsed().as_millis();
    let ask_ctx = if diversify_documents {
        build_ask_context_from_hits(cfg, &result.hits, retrieval_elapsed_ms)
    } else {
        build_ask_context_from_hits_in_retrieval_order(cfg, &result.hits, retrieval_elapsed_ms)
    };
    Ok((ask_ctx, result.hits))
}

/// Assemble an [`AskContext`] from the retrieval hits, formatting the context
/// string in the exact shape the synthesis prompt expects.
fn build_ask_context_from_hits(
    cfg: &Config,
    hits: &[QueryServiceHit],
    retrieval_elapsed_ms: u128,
) -> AskContext {
    let ordered_hits = document_diverse_hit_order(hits);
    build_ask_context_from_ordered_hits(cfg, &ordered_hits, retrieval_elapsed_ms)
}

/// Keep the highest-ranked chunk from every canonical document ahead of
/// repeated chunks from documents already represented. This preserves
/// retrieval rank within both passes while ensuring the synthesis context does
/// not spend most of its bounded window on a single high-volume document.
fn document_diverse_hit_order(hits: &[QueryServiceHit]) -> Vec<&QueryServiceHit> {
    let mut seen_documents = BTreeSet::new();
    let mut first_chunks = Vec::with_capacity(hits.len());
    let mut repeated_chunks = Vec::new();
    for hit in hits {
        let identity = normalize::canonical_source_identity(&display_source(&hit.canonical_uri));
        if seen_documents.insert(identity) {
            first_chunks.push(hit);
        } else {
            repeated_chunks.push(hit);
        }
    }
    first_chunks.extend(repeated_chunks);
    first_chunks
}

/// Explain traces retain the retrieval-order prefix contract documented in
/// `ask_retrieval::explain`; normal Ask/Evaluate synthesis uses the
/// document-diverse ordering above.
fn build_ask_context_from_hits_in_retrieval_order(
    cfg: &Config,
    hits: &[QueryServiceHit],
    retrieval_elapsed_ms: u128,
) -> AskContext {
    build_ask_context_from_ordered_hits(cfg, &hits.iter().collect::<Vec<_>>(), retrieval_elapsed_ms)
}

fn build_ask_context_from_ordered_hits(
    cfg: &Config,
    ordered_hits: &[&QueryServiceHit],
    retrieval_elapsed_ms: u128,
) -> AskContext {
    let chunk_limit = cfg
        .ask_chunk_limit
        .clamp(1, axon_api::MAX_CANONICAL_CITATIONS);
    let max_context_chars = cfg.ask_max_context_chars;

    let mut context = String::from(CONTEXT_PREFIX);
    let mut selected_urls: Vec<String> = Vec::new();
    let mut selected_citations = Vec::new();
    let mut domains: BTreeSet<String> = BTreeSet::new();

    for (zero_based_source_idx, hit) in ordered_hits.iter().take(chunk_limit).enumerate() {
        let source_idx = zero_based_source_idx + 1;
        let source = display_source(&hit.canonical_uri);
        let header = format!("## Top Chunk [S{}]: {}\n\n", source_idx, source);
        let body = defang_chunk_text(&hit.text);
        let entry = wrap_retrieved_content(&header, &body);
        let sep_len = if source_idx == 1 {
            0
        } else {
            CONTEXT_SEPARATOR.len()
        };
        if context.len() + sep_len + entry.len() > max_context_chars {
            break;
        }
        if source_idx > 1 {
            context.push_str(CONTEXT_SEPARATOR);
        }
        context.push_str(&entry);
        if let Some(host) = reqwest::Url::parse(&hit.canonical_uri)
            .ok()
            .and_then(|u| u.host_str().map(ToString::to_string))
        {
            domains.insert(host);
        }
        selected_urls.push(hit.canonical_uri.clone());
        selected_citations.push(hit.citation.clone());
    }

    let chunks_selected = selected_urls.len();
    let mut ask_ctx = AskContext::from_retrieval(
        context,
        chunks_selected,
        chunks_selected,
        retrieval_elapsed_ms,
        domains.into_iter().collect(),
        &selected_urls,
        Vec::new(),
    );
    ask_ctx.citations = selected_citations;
    ask_ctx
}

/// Wrap a retrieved-chunk body in the XML trust boundary + axon header, matching
/// the legacy ask context builder so the synthesis prompt treats the enclosed
/// content as untrusted indexed evidence.
fn wrap_retrieved_content(header: &str, body: &str) -> String {
    format!("{header}<retrieved_content trust=\"evidence_only\">\n{body}\n</retrieved_content>")
}

/// Defang structural markers so indexed content cannot forge citation keys
/// (`[S#]`) or source-section headers into the synthesis context. Mirrors the
/// legacy `axon_vector` ask defang exactly (zero-width space breaks recognition
/// without altering visible text).
fn defang_chunk_text(text: &str) -> String {
    let s = text
        .replace("## Sources", "## \u{200b}Sources")
        .replace("## Source Document", "## \u{200b}Source Document")
        .replace("## Top Chunk", "## \u{200b}Top Chunk")
        .replace("## Supplemental Chunk", "## \u{200b}Supplemental Chunk");
    defang_citation_patterns(&s)
}

fn defang_citation_patterns(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(pos) = rest.find("[S") {
        result.push_str(&rest[..pos]);
        let tail = &rest[pos + 2..];
        let digit_end = tail.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digit_end > 0 && tail[digit_end..].starts_with(']') {
            result.push_str("[\u{200b}S");
            result.push_str(&tail[..digit_end]);
            result.push(']');
            rest = &tail[digit_end + 1..];
        } else {
            result.push_str("[S");
            rest = tail;
        }
    }
    result.push_str(rest);
    result
}

/// Derive the source identity rendered into synthesis context.
///
/// Web sources retain the compact host label. Session documents must retain
/// their opaque canonical document URI: reducing every
/// `session://<provider>/doc_session_<hash>` URI to `<provider>` makes
/// distinct documents indistinguishable to citation normalization. Legacy or
/// malformed session URIs still fall back to host-only rendering so local
/// transcript identity cannot cross the synthesis boundary.
fn display_source(uri: &str) -> String {
    let Ok(url) = reqwest::Url::parse(uri) else {
        return uri.to_string();
    };
    if url.scheme() == "session"
        && url
            .path()
            .strip_prefix("/doc_session_")
            .is_some_and(|id| id.len() == 24 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
        && url.query().is_none()
        && url.fragment().is_none()
    {
        return uri.to_string();
    }
    url.host_str()
        .map(ToString::to_string)
        .unwrap_or_else(|| uri.to_string())
}

#[cfg(test)]
#[path = "ask_retrieval_tests.rs"]
mod tests;
