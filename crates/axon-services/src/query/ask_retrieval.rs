//! `ask` retrieval half routed through the new `axon-retrieval` engine.
//!
//! Issue #298 cutover: the SEARCH + CONTEXT portion of `ask` now embeds +
//! hybrid-searches through [`axon_retrieval::run_query`] (dense + bm42 RRF)
//! instead of the legacy `axon_vector::ops::commands::ask::build_ask_context`
//! reranker + full-doc fetcher. The retrieved chunks are formatted into the
//! same `Sources:\n ## Top Chunk [S#]: …` context string the synthesis prompt
//! expects, wrapped in the `AskContext`, and handed to the UNCHANGED synthesis
//! pipeline (`axon_vector::ops::commands::ask::ask_result_from_context`), which
//! keeps the existing Gemini/core-llm completion, citation validation, and
//! result assembly.
//!
//! The LLM synthesis half is deliberately left on the legacy path per the slice
//! scope; only retrieval + context-build were cut over.

use std::collections::BTreeSet;
use std::error::Error;
use std::sync::Arc;

use axon_core::config::Config;
use axon_core::error::ServiceError;
use axon_core::logging::log_info;
use axon_embedding::provider::EmbeddingProvider;
use axon_retrieval::{QueryServiceHit, QueryServiceRequest, run_query};
use axon_vector::ops::commands::ask::ask_result_from_context_with_deltas;
use axon_vector::ops::commands::ask::{AskContext, ask_result, ask_result_from_context};
use axon_vectors::store::VectorStore;

use crate::context::{ServiceContext, build_read_stores_from_config};
use crate::types::AskResult;

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
    // Explain mode traces the LEGACY reranker's per-candidate decisions
    // (`AskResult.explain`), which the retrieval engine does not produce. The
    // #298 cutover replaces only the normal ask retrieval path; explain-ask
    // (used by `train`) stays on the legacy `build_ask_context` reranker so its
    // candidate trace remains available.
    if cfg.ask_explain {
        return ask_result(cfg, question)
            .await
            .map_err(|e| -> Box<dyn Error> {
                Box::new(ServiceError::new(format!(
                    "ask (explain) failed for {}: {e}",
                    question.chars().take(80).collect::<String>()
                )))
            });
    }

    if cfg.qdrant_url.trim().is_empty() || cfg.tei_url.trim().is_empty() {
        return Err(Box::new(ServiceError::new(
            "ask requires both QDRANT_URL and TEI_URL to be configured for the retrieval engine"
                .to_string(),
        )));
    }

    let ask_started = std::time::Instant::now();
    let ask_ctx = retrieval_ask_context(ctx, cfg, question, "ask").await?;

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
    if cfg.qdrant_url.trim().is_empty() || cfg.tei_url.trim().is_empty() {
        return Err(Box::new(ServiceError::new(format!(
            "{label} requires both QDRANT_URL and TEI_URL to be configured for the retrieval engine"
        ))));
    }

    let retrieval_started = std::time::Instant::now();
    let (store, provider, provider_id, model, dimensions) = resolve_stores(ctx, cfg);

    // The ask/evaluate path fetches a wider candidate pool than plain `query`
    // before trimming to the context entries synthesis will read.
    // `ask_hybrid_candidates` (env `AXON_ASK_HYBRID_CANDIDATES`, default 150) is
    // the fetch width; `ask_chunk_limit` (default 24) bounds the entries
    // rendered into context.
    let fetch_limit = cfg.ask_hybrid_candidates.max(cfg.ask_chunk_limit).max(1) as u32;

    log_info(&format!(
        "{label} retrieval: axon-retrieval engine collection={} fetch_limit={} chunk_limit={}",
        cfg.collection, fetch_limit, cfg.ask_chunk_limit,
    ));

    let result = run_query(
        store,
        provider,
        provider_id,
        model,
        dimensions,
        QueryServiceRequest {
            query: question.to_string(),
            collection: cfg.collection.clone(),
            limit: fetch_limit,
        },
    )
    .await
    .map_err(|e| -> Box<dyn Error> {
        Box::new(ServiceError::new(format!(
            "{label} retrieval failed for {}: {e}",
            question.chars().take(80).collect::<String>()
        )))
    })?;

    let retrieval_elapsed_ms = retrieval_started.elapsed().as_millis();
    Ok(build_ask_context_from_hits(
        cfg,
        result.hits,
        retrieval_elapsed_ms,
    ))
}

/// Assemble an [`AskContext`] from the retrieval hits, formatting the context
/// string in the exact shape the synthesis prompt expects.
fn build_ask_context_from_hits(
    cfg: &Config,
    hits: Vec<QueryServiceHit>,
    retrieval_elapsed_ms: u128,
) -> AskContext {
    let chunk_limit = cfg.ask_chunk_limit.max(1);
    let max_context_chars = cfg.ask_max_context_chars;

    let mut context = String::from(CONTEXT_PREFIX);
    let mut selected_urls: Vec<String> = Vec::new();
    let mut domains: BTreeSet<String> = BTreeSet::new();
    let mut source_idx = 1usize;

    for hit in hits.into_iter().take(chunk_limit) {
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
        selected_urls.push(hit.canonical_uri);
        source_idx += 1;
    }

    let chunks_selected = selected_urls.len();
    AskContext::from_retrieval(
        context,
        chunks_selected,
        chunks_selected,
        retrieval_elapsed_ms,
        domains.into_iter().collect(),
        &selected_urls,
        Vec::new(),
    )
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

type ResolvedStores = (
    Arc<dyn VectorStore>,
    Arc<dyn EmbeddingProvider>,
    axon_api::source::ProviderId,
    String,
    u32,
);

/// Resolve the read-plane stores + provider identity, preferring the context's
/// attached runtime (`serve`/`mcp`/`--wait`); otherwise build from `cfg`.
fn resolve_stores(ctx: &ServiceContext, cfg: &Config) -> ResolvedStores {
    if let Some(target) = ctx.target_local_source_runtime() {
        return (
            Arc::clone(&target.vector_store),
            Arc::clone(&target.embedding_provider),
            target.embedding_provider_id.clone(),
            target.embedding_model.clone(),
            target.embedding_dimensions,
        );
    }
    let stores = build_read_stores_from_config(cfg);
    (
        stores.vector_store,
        stores.embedding_provider,
        stores.embedding_provider_id,
        stores.embedding_model,
        stores.embedding_dimensions,
    )
}

/// Derive a short display source (host, or the raw value) from a canonical URI.
fn display_source(uri: &str) -> String {
    reqwest::Url::parse(uri)
        .ok()
        .and_then(|url| url.host_str().map(ToString::to_string))
        .unwrap_or_else(|| uri.to_string())
}

#[cfg(test)]
#[path = "ask_retrieval_tests.rs"]
mod tests;
