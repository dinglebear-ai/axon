//! Ask retrieval and context assembly through axon-retrieval.

use std::collections::BTreeSet;
use std::error::Error;

use axon_api::source::{AuthSnapshot, OperationKind};
use axon_core::config::Config;
use axon_core::error::ServiceError;
use axon_core::logging::log_info;
use axon_retrieval::{QueryServiceRequest, run_query};

use super::provider_execution::ReadExecution;
use super::synthesis::assemble::assemble_explain_result;
use super::synthesis::normalize;
use super::synthesis::{AskContext, ask_result_from_context, ask_result_from_context_with_deltas};
use crate::context::ServiceContext;
use crate::types::AskResult;

mod explain;
mod ranking;

const CONTEXT_PREFIX: &str = "Sources:
";
const CONTEXT_SEPARATOR: &str = "

---

";
const MIN_USEFUL_BODY_CHARS: usize = 240;

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
    validate_retrieval_config(cfg, "ask")?;
    if !cfg.ask_explain {
        super::synthesis::validate_ask_llm_config(cfg).map_err(|error| -> Box<dyn Error> {
            Box::new(ServiceError::new(format!(
                "ask LLM configuration is invalid: {error}"
            )))
        })?;
    }

    let ask_started = std::time::Instant::now();
    if cfg.ask_explain {
        let (ask_ctx, ranked) =
            retrieval_ask_context_with_ranking(ctx, cfg, question, "ask", auth_snapshot.clone())
                .await?;
        let trace = explain::build_explain_trace(cfg, question, &ranked, &ask_ctx.context);
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
        Some(callback) => {
            ask_result_from_context_with_deltas(cfg, question, ask_ctx, ask_started, callback).await
        }
        None => ask_result_from_context(cfg, question, ask_ctx, ask_started).await,
    };
    synth.map_err(|error| -> Box<dyn Error> {
        Box::new(ServiceError::new(format!(
            "ask synthesis failed for {}: {error}",
            question.chars().take(80).collect::<String>()
        )))
    })
}

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
    let (ask_ctx, _ranked) =
        retrieval_ask_context_with_ranking(ctx, cfg, question, label, auth_snapshot).await?;
    Ok(ask_ctx)
}

async fn retrieval_ask_context_with_ranking(
    ctx: &ServiceContext,
    cfg: &Config,
    question: &str,
    label: &str,
    auth_snapshot: Option<AuthSnapshot>,
) -> Result<(AskContext, ranking::RankingResult), Box<dyn Error>> {
    validate_retrieval_config(cfg, label)?;
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
    .await
    .map_err(|error| -> Box<dyn Error> { error })?;

    let store = execution.scheduled_vectors();
    let provider = execution.scheduled_embedding();
    let provider_id = execution.embedding_provider_id();
    let model = execution.embedding_model();
    let dimensions = execution.embedding_dimensions();
    let fetch_limit =
        u32::try_from(cfg.ask_candidate_limit.max(cfg.ask_chunk_limit).max(1)).unwrap_or(u32::MAX);
    let (since, before) = super::retrieval::normalize_time_bounds(cfg, chrono::Utc::now())?;

    log_info(&format!(
        "{label} retrieval: collection={} result_limit={} hybrid_candidates={} configured_chunk_limit={}",
        cfg.collection, fetch_limit, cfg.ask_hybrid_candidates, cfg.ask_chunk_limit,
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
            hybrid_candidates: cfg
                .hybrid_search_enabled
                .then(|| u32::try_from(cfg.ask_hybrid_candidates.max(1)).unwrap_or(u32::MAX)),
            since,
            before,
        },
    )
    .await;
    execution.finish(ctx, &raw_result).await;
    let result = raw_result.map_err(|error| -> Box<dyn Error> {
        Box::new(ServiceError::new(format!(
            "{label} retrieval failed for {}: {error}",
            question.chars().take(80).collect::<String>()
        )))
    })?;

    let retrieval_elapsed_ms = retrieval_started.elapsed().as_millis();
    let mut ranked =
        ranking::rank_candidates(cfg, question, result.hits, cfg.hybrid_search_enabled);
    if ranked.ranked_indices.is_empty() {
        return Err(Box::new(ServiceError::new(if cfg.hybrid_search_enabled {
            "ask retrieval returned candidates, but none passed topical/source-quality filtering"
                .to_string()
        } else {
            format!(
                "ask retrieval returned no candidates above relevance threshold {:.3}",
                cfg.ask_min_relevance_score
            )
        })));
    }

    let ask_ctx = build_ask_context_from_ranking(cfg, &mut ranked, retrieval_elapsed_ms);
    Ok((ask_ctx, ranked))
}

fn validate_retrieval_config(cfg: &Config, label: &str) -> Result<(), Box<dyn Error>> {
    if cfg.qdrant_url.trim().is_empty() || cfg.tei_url.trim().is_empty() {
        return Err(Box::new(ServiceError::new(format!(
            "{label} requires both QDRANT_URL and TEI_URL to be configured for the retrieval engine"
        ))));
    }
    Ok(())
}

fn build_ask_context_from_ranking(
    cfg: &Config,
    ranked: &mut ranking::RankingResult,
    retrieval_elapsed_ms: u128,
) -> AskContext {
    let context_started = std::time::Instant::now();
    let ordered_indices = document_diverse_ranked_order(ranked);
    let budget = ranked.effective_budget;
    let chunk_limit = budget
        .chunk_limit
        .clamp(1, axon_api::MAX_CANONICAL_CITATIONS);
    let mut context = String::from(CONTEXT_PREFIX);
    let mut context_chars = CONTEXT_PREFIX.chars().count();
    let mut selected_urls = Vec::new();
    let mut selected_citations = Vec::new();
    let mut warnings = Vec::new();
    if legacy_full_doc_controls_configured(cfg) {
        warnings.push(
            "legacy ask full-document/cache controls are retained for configuration compatibility but are not executed by the unified retrieval-engine path; bounded chunk context is active instead"
                .to_string(),
        );
    }
    let mut body_truncations = 0usize;
    let mut budget_skips = 0usize;

    for candidate_index in ordered_indices {
        if selected_urls.len() >= chunk_limit {
            break;
        }
        let source_idx = selected_urls.len() + 1;
        let candidate = &mut ranked.candidates[candidate_index];
        let source = display_source(&candidate.hit.canonical_uri);
        let header = format!(
            "## Top Chunk [S{source_idx}]: {source}

"
        );
        let separator_chars = if source_idx == 1 {
            0
        } else {
            CONTEXT_SEPARATOR.chars().count()
        };
        let wrapper_chars = r#"<retrieved_content trust="evidence_only">

</retrieved_content>"#
            .chars()
            .count();
        let fixed_chars = separator_chars + header.chars().count() + wrapper_chars;
        let remaining = budget.max_context_chars.saturating_sub(context_chars);
        if remaining <= fixed_chars + MIN_USEFUL_BODY_CHARS {
            candidate.selection_reason = Some(
                "skipped: effective context budget had insufficient room for a useful chunk"
                    .to_string(),
            );
            budget_skips += 1;
            continue;
        }

        let body_budget = budget
            .max_chunk_chars
            .min(remaining.saturating_sub(fixed_chars));
        let (body, truncated) =
            clip_text_chars(&defang_chunk_text(&candidate.hit.text), body_budget);
        if body.chars().count() < MIN_USEFUL_BODY_CHARS
            && candidate.hit.text.chars().count() >= MIN_USEFUL_BODY_CHARS
        {
            candidate.selection_reason =
                Some("skipped: remaining context budget would over-truncate the chunk".to_string());
            budget_skips += 1;
            continue;
        }
        if truncated {
            body_truncations += 1;
        }
        let entry = wrap_retrieved_content(&header, &body);
        if source_idx > 1 {
            context.push_str(CONTEXT_SEPARATOR);
            context_chars += separator_chars;
        }
        context.push_str(&entry);
        context_chars += entry.chars().count();
        candidate.selected_context_rank = Some(source_idx);
        candidate.selection_reason = Some(if truncated {
            "selected after per-chunk context truncation".to_string()
        } else {
            "selected".to_string()
        });
        selected_urls.push(candidate.hit.canonical_uri.clone());
        selected_citations.push(candidate.hit.citation.clone());
    }

    if body_truncations > 0 {
        warnings.push(format!(
            "ask context truncated {body_truncations} oversized chunk(s) to preserve source diversity"
        ));
    }
    if budget_skips > 0 {
        warnings.push(format!(
            "ask context skipped {budget_skips} candidate(s) that could not fit the effective context budget"
        ));
    }

    let chunks_selected = selected_urls.len();
    let reranked_count = ranked.ranked_indices.len();
    let candidate_count = ranked.candidates.len();
    let mut ask_ctx = AskContext::from_retrieval(
        context,
        candidate_count,
        chunks_selected,
        retrieval_elapsed_ms,
        ranked.top_domains.clone(),
        &selected_urls,
        warnings,
    );
    ask_ctx.reranked_count = reranked_count;
    ask_ctx.citations = selected_citations;
    ask_ctx.authoritative_ratio = ranked
        .configured_authority_ratio
        .max(ranked.product_authority_ratio);
    ask_ctx.configured_authority_ratio = ranked.configured_authority_ratio;
    ask_ctx.product_authority_ratio = ranked.product_authority_ratio;
    ask_ctx.detected_complexity = ranked.complexity.as_str();
    ask_ctx.context_elapsed_ms = context_started.elapsed().as_millis();
    ask_ctx.effective_chunk_limit = budget.chunk_limit;
    ask_ctx.effective_max_context_chars = budget.max_context_chars;
    ask_ctx.max_chunk_chars = budget.max_chunk_chars;
    ask_ctx.full_doc_fetch_skip_reason = "not_supported_by_retrieval_engine";
    ask_ctx.full_docs_source = "not_supported_by_retrieval_engine";
    ask_ctx
}

fn document_diverse_ranked_order(ranked: &ranking::RankingResult) -> Vec<usize> {
    let mut seen_documents = BTreeSet::new();
    let mut first_chunks = Vec::with_capacity(ranked.ranked_indices.len());
    let mut repeated_chunks = Vec::new();
    for index in ranked.ranked_indices.iter().copied() {
        let identity =
            normalize::canonical_source_identity(&ranked.candidates[index].hit.canonical_uri);
        if seen_documents.insert(identity) {
            first_chunks.push(index);
        } else {
            repeated_chunks.push(index);
        }
    }
    first_chunks.extend(repeated_chunks);
    first_chunks
}

fn legacy_full_doc_controls_configured(cfg: &Config) -> bool {
    let defaults = Config::default();
    cfg.ask_full_docs_explicit
        || cfg.ask_full_docs != defaults.ask_full_docs
        || cfg.ask_backfill_chunks != defaults.ask_backfill_chunks
        || cfg.ask_doc_fetch_concurrency != defaults.ask_doc_fetch_concurrency
        || cfg.ask_doc_chunk_limit != defaults.ask_doc_chunk_limit
        || cfg.ask_cache_enabled
        || cfg.ask_fulldoc_skip_enabled
}

fn wrap_retrieved_content(header: &str, body: &str) -> String {
    format!(
        r#"{header}<retrieved_content trust="evidence_only">
{body}
</retrieved_content>"#
    )
}

fn clip_text_chars(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_string(), false);
    }
    const MARKER: &str = "
[chunk truncated to context budget]";
    let marker_chars = MARKER.chars().count();
    let content_chars = max_chars.saturating_sub(marker_chars);
    let mut clipped = text.chars().take(content_chars).collect::<String>();
    clipped.push_str(MARKER);
    (clipped, true)
}

fn defang_chunk_text(text: &str) -> String {
    let source_headers_defanged = text
        .replace("## Sources", "## ​Sources")
        .replace("## Source Document", "## ​Source Document")
        .replace("## Top Chunk", "## ​Top Chunk")
        .replace("## Supplemental Chunk", "## ​Supplemental Chunk");
    defang_citation_patterns(&source_headers_defanged)
}

fn defang_citation_patterns(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(pos) = rest.find("[S") {
        result.push_str(&rest[..pos]);
        let tail = &rest[pos + 2..];
        let digit_end = tail
            .bytes()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if digit_end > 0 && tail[digit_end..].starts_with(']') {
            result.push_str("[​S");
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

fn display_source(uri: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(uri) else {
        return uri.to_string();
    };
    match url.scheme() {
        "http" | "https" => {
            url.set_fragment(None);
            url.to_string()
        }
        "session"
            if url.path().strip_prefix("/doc_session_").is_some_and(|id| {
                id.len() == 24 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
            }) && url.query().is_none()
                && url.fragment().is_none() =>
        {
            uri.to_string()
        }
        _ => url
            .host_str()
            .map(ToString::to_string)
            .unwrap_or_else(|| uri.to_string()),
    }
}

#[cfg(test)]
#[path = "ask_retrieval_tests.rs"]
mod tests;
