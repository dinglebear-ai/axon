use crate::core::ask_explain::{CorpusHealthDiagnostic, CorpusHealthKind};
use crate::core::config::Config;
use crate::core::llm;
use crate::core::logging::{log_info, log_warn};

mod context;
mod normalize;
mod output;
#[cfg(test)]
#[path = "ask_stream_correction_tests.rs"]
mod stream_correction_tests;
pub(crate) mod synthesis_prompt;
#[cfg(test)]
#[path = "ask_tests.rs"]
mod tests;
pub(crate) mod timing;

pub(crate) use context::{AskContext, build_ask_context};
pub(crate) use normalize::{normalize_ask_answer, summarize_citation_validation};
pub(crate) use timing::{AskTiming, AskTimingSlot};

pub(super) fn validate_ask_llm_config(cfg: &Config) -> anyhow::Result<()> {
    let backend = llm::LlmBackendConfig::from_config(cfg);
    match backend.kind {
        llm::LlmBackendKind::GeminiHeadless => {
            llm::headless::gemini::validate_config(&backend).map_err(|e| anyhow::anyhow!("{e}"))
        }
        llm::LlmBackendKind::OpenAiCompat => {
            backend
                .openai_base_url
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("AXON_OPENAI_BASE_URL is required for ask"))?;
            backend
                .openai_model
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("AXON_OPENAI_MODEL is required for ask"))?;
            Ok(())
        }
    }
}

pub async fn ask_payload(cfg: &Config, query: &str) -> anyhow::Result<serde_json::Value> {
    ask_payload_with_delta_handler(cfg, query, Option::<fn(&str)>::None).await
}

pub async fn ask_payload_with_deltas<F>(
    cfg: &Config,
    query: &str,
    on_delta: F,
) -> anyhow::Result<serde_json::Value>
where
    F: FnMut(&str) + Send,
{
    ask_payload_with_delta_handler(cfg, query, Some(on_delta)).await
}

async fn ask_payload_with_delta_handler<F>(
    cfg: &Config,
    query: &str,
    mut on_delta: Option<F>,
) -> anyhow::Result<serde_json::Value>
where
    F: FnMut(&str) + Send,
{
    let ask_started = std::time::Instant::now();
    let diagnostics_enabled = cfg.ask_diagnostics || cfg.ask_explain;
    let mut timing = AskTiming::new(diagnostics_enabled, ask_started);

    log_info(&format!(
        "ask start query_len={} collection={} backend={:?} candidate_limit={} hybrid_candidates={} chunk_limit={} full_docs={} doc_chunk_limit={} max_context_chars={}",
        query.len(),
        cfg.collection,
        cfg.llm_backend,
        cfg.ask_candidate_limit,
        cfg.ask_hybrid_candidates,
        cfg.ask_chunk_limit,
        cfg.ask_full_docs,
        cfg.ask_doc_chunk_limit,
        cfg.ask_max_context_chars,
    ));
    let ctx = match build_ask_context(cfg, query, &mut timing).await {
        Ok(ctx) => ctx,
        Err(err) if can_answer_from_follow_up_history(cfg, &err) => {
            history_only_ask_context(ask_started.elapsed().as_millis())
        }
        Err(err) => return Err(err),
    };
    log_info(&format!(
        "ask context ready candidates={} reranked={} chunks={} full_docs={} supplemental={} context_chars={} retrieval_ms={} context_ms={}",
        ctx.candidate_count,
        ctx.reranked_count,
        ctx.chunks_selected,
        ctx.full_docs_selected,
        ctx.supplemental_count,
        ctx.context.len(),
        ctx.retrieval_elapsed_ms,
        ctx.context_elapsed_ms,
    ));
    if cfg.ask_explain {
        return Ok(build_explain_payload(
            cfg,
            query,
            &ctx,
            diagnostics_enabled,
            &timing,
            ask_started.elapsed().as_millis(),
        ));
    }

    let context = ask_context_with_follow_up(cfg, &ctx.context);
    let (answer, llm_total_ms) =
        resolve_answer_and_timing(cfg, query, &context, on_delta.take(), &mut timing).await?;
    Ok(assemble_ask_payload(
        cfg,
        query,
        &ctx,
        &answer,
        llm_total_ms,
        ask_started.elapsed().as_millis(),
        &timing,
        diagnostics_enabled,
    ))
}

/// Run the LLM step and normalise the answer text.  Called by the non-explain
/// path only; returns the normalised answer and the raw LLM wall-clock ms.
async fn resolve_answer_and_timing<F>(
    cfg: &Config,
    query: &str,
    context: &str,
    on_delta: Option<F>,
    timing: &mut AskTiming,
) -> anyhow::Result<(String, u128)>
where
    F: FnMut(&str) + Send,
{
    validate_ask_llm_config(cfg)?;
    log_info(&format!(
        "ask llm starting backend={:?} model={} context_chars={} stream={}",
        cfg.llm_backend,
        llm::configured_model_from_config(cfg).unwrap_or_else(|| "<default>".to_string()),
        context.len(),
        cfg.ask_stream,
    ));
    let llm = if let Some(callback) = on_delta {
        output::ask_llm_answer_with_deltas(cfg, query, context, callback).await
    } else {
        output::ask_llm_answer(cfg, query, context).await
    }
    .map_err(|e| anyhow::anyhow!("LLM answer generation failed: {e}"))?;

    let (answer_text, mut llm_total_ms) = match &llm {
        output::AskLlmCompletion::Streamed {
            answer,
            ttft_at,
            llm_total_ms,
        } => {
            timing.set_streamed(true);
            // TTFT is measured from the outer ask request_start so retrieval
            // and context construction are included in user-visible latency.
            if let Some(start) = timing.request_start() {
                let ttft_ms = ttft_at.saturating_duration_since(start).as_millis();
                timing.set_ttft(ttft_ms);
            }
            (answer.as_str(), *llm_total_ms)
        }
        output::AskLlmCompletion::Fallback {
            answer,
            llm_total_ms,
        } => {
            timing.set_streamed(false);
            (answer.as_str(), *llm_total_ms)
        }
    };
    timing.set(AskTimingSlot::LlmTotal, llm_total_ms);

    let normalize_started = std::time::Instant::now();
    let mut answer = normalize_ask_answer(cfg, query, answer_text, context);
    let validation = summarize_citation_validation(&answer);
    if !validation.valid {
        let repair_started = std::time::Instant::now();
        log_warn(&format!(
            "ask citation validation failed; retrying once with repair prompt: {:?}",
            validation.issues
        ));
        if let Some(repaired) =
            retry_answer_for_citation_validation(cfg, query, context, &answer, &validation).await?
        {
            answer = repaired;
            llm_total_ms += repair_started.elapsed().as_millis();
            timing.set(AskTimingSlot::LlmTotal, llm_total_ms);
        }
    }
    timing.record(AskTimingSlot::Normalize, normalize_started);
    if cfg.ask_stream && !cfg.json_output && !cfg.ask_explain && answer.trim() != answer_text.trim()
    {
        print_normalized_stream_correction(&answer);
    }
    Ok((answer, llm_total_ms))
}

async fn retry_answer_for_citation_validation(
    cfg: &Config,
    query: &str,
    context: &str,
    invalid_answer: &str,
    validation: &normalize::CitationValidationSummary,
) -> anyhow::Result<Option<String>> {
    let mut repair_cfg = cfg.clone();
    repair_cfg.ask_stream = false;
    repair_cfg.json_output = true;
    let repair_query = format!(
        "{query}\n\nYour previous answer failed Axon citation validation:\n{}\n\nRewrite the answer from scratch using the retrieved context. Cite at least two distinct source documents when the context provides them, and keep every factual claim grounded in [S#] citations.\n\nPrevious invalid answer:\n{}",
        validation
            .issues
            .iter()
            .map(|issue| format!("- {issue}"))
            .collect::<Vec<_>>()
            .join("\n"),
        invalid_answer.trim()
    );
    let repaired = output::ask_llm_answer(&repair_cfg, &repair_query, context)
        .await
        .map_err(|e| anyhow::anyhow!("citation repair retry failed: {e}"))?;
    let repaired_answer_text = match repaired {
        output::AskLlmCompletion::Streamed { answer, .. }
        | output::AskLlmCompletion::Fallback { answer, .. } => answer,
    };
    let normalized = normalize_ask_answer(cfg, query, &repaired_answer_text, context);
    let repaired_validation = summarize_citation_validation(&normalized);
    if repaired_validation.valid {
        log_info(&format!(
            "ask citation repair succeeded canonical_citations={}",
            repaired_validation.canonical_citation_count
        ));
        Ok(Some(normalized))
    } else {
        log_warn(&format!(
            "ask citation repair still failed: {:?}",
            repaired_validation.issues
        ));
        Ok(None)
    }
}

/// Build the final JSON payload for a completed ask (non-explain path).
#[allow(clippy::too_many_arguments)]
fn assemble_ask_payload(
    cfg: &Config,
    query: &str,
    ctx: &AskContext,
    answer: &str,
    llm_total_ms: u128,
    total_elapsed_ms: u128,
    timing: &AskTiming,
    diagnostics_enabled: bool,
) -> serde_json::Value {
    log_info(&format!(
        "ask complete answer_chars={} llm_ms={} total_ms={}",
        answer.len(),
        llm_total_ms,
        total_elapsed_ms,
    ));
    serde_json::json!({
        "query": query,
        "answer": answer,
        "citation_validation": summarize_citation_validation(answer),
        "session": serde_json::Value::Null,
        "warnings": &ctx.warnings,
        "diagnostics": build_diagnostics_json(diagnostics_enabled, cfg, ctx),
        "explain": serde_json::Value::Null,
        "timing_ms": build_timing_json(
            ctx.retrieval_elapsed_ms,
            ctx.context_elapsed_ms,
            llm_total_ms,
            total_elapsed_ms,
            timing,
        ),
    })
}

fn build_explain_payload(
    cfg: &Config,
    query: &str,
    ctx: &AskContext,
    diagnostics_enabled: bool,
    timing: &AskTiming,
    total_elapsed_ms: u128,
) -> serde_json::Value {
    log_info(&format!(
        "ask explain complete total_ms={total_elapsed_ms} context_chars={}",
        ctx.context.len()
    ));
    serde_json::json!({
        "query": query,
        "answer": "",
        "session": serde_json::Value::Null,
        "warnings": &ctx.warnings,
        "diagnostics": build_diagnostics_json(diagnostics_enabled, cfg, ctx),
        "explain": ctx.explain,
        "timing_ms": build_timing_json(
            ctx.retrieval_elapsed_ms,
            ctx.context_elapsed_ms,
            0,
            total_elapsed_ms,
            timing,
        ),
    })
}

fn build_diagnostics_json(enabled: bool, cfg: &Config, ctx: &AskContext) -> serde_json::Value {
    if !enabled {
        return serde_json::Value::Null;
    }
    serde_json::json!({
        "candidate_pool": ctx.candidate_count,
        "reranked_pool": ctx.reranked_count,
        "chunks_selected": ctx.chunks_selected,
        "full_docs_selected": ctx.full_docs_selected,
        "supplemental_selected": ctx.supplemental_count,
        "context_chars": ctx.context.len(),
        "min_relevance_score": cfg.ask_min_relevance_score,
        "ask_candidate_limit": cfg.ask_candidate_limit,
        "ask_chunk_limit": cfg.ask_chunk_limit,
        "ask_backfill_chunks": cfg.ask_backfill_chunks,
        "ask_doc_chunk_limit": cfg.ask_doc_chunk_limit,
        "ask_hybrid_candidates": cfg.ask_hybrid_candidates,
        "ask_full_docs_configured": cfg.ask_full_docs,
        "ask_full_docs_explicit": cfg.ask_full_docs_explicit,
        "ask_fulldoc_skip_enabled": cfg.ask_fulldoc_skip_enabled,
        "ask_max_context_chars": cfg.ask_max_context_chars,
        "doc_fetch_concurrency": cfg.ask_doc_fetch_concurrency,
        "top_domains": &ctx.top_domains,
        "authority_ratio": ctx.authoritative_ratio,
        "configured_authority_ratio": ctx.configured_authority_ratio,
        "product_authority_ratio": ctx.product_authority_ratio,
        "corpus_health": &ctx.corpus_health,
        "full_doc_fetch_skipped": ctx.full_doc_fetch_skipped,
        "full_doc_fetch_skip_reason": ctx.full_doc_fetch_skip_reason,
        "full_doc_fetch_errors": &ctx.full_doc_fetch_errors,
        "detected_complexity": ctx.detected_complexity,
        "resolved_full_docs": ctx.resolved_full_docs,
        "full_docs_source": ctx.full_docs_source,
    })
}

fn print_normalized_stream_correction(answer: &str) {
    println!("{}", normalized_stream_correction_text(answer));
}

fn normalized_stream_correction_text(answer: &str) -> String {
    format!("\n\n---\n\nNormalized answer (stored for JSON and follow-up sessions):\n\n{answer}")
}

fn ask_context_with_follow_up(cfg: &Config, context: &str) -> String {
    let Some(history) = cfg
        .ask_follow_up_context
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return context.to_string();
    };
    if context.trim().is_empty() {
        format!("Sources:\n{history}")
    } else {
        format!("{context}\n\n---\n\n{history}")
    }
}

fn can_answer_from_follow_up_history(cfg: &Config, err: &anyhow::Error) -> bool {
    cfg.ask_follow_up_context
        .as_deref()
        .map(str::trim)
        .is_some_and(|history| !history.is_empty())
        && {
            let message = err.to_string();
            message.contains("No candidates passed topical overlap")
                || message.contains("Failed to retrieve any context sources for ask")
        }
}

fn history_only_ask_context(elapsed_ms: u128) -> AskContext {
    AskContext {
        context: String::new(),
        candidate_count: 0,
        reranked_count: 0,
        chunks_selected: 0,
        full_docs_selected: 0,
        supplemental_count: 0,
        retrieval_elapsed_ms: elapsed_ms,
        context_elapsed_ms: 0,
        diagnostic_sources: Vec::new(),
        top_domains: Vec::new(),
        authoritative_ratio: 0.0,
        configured_authority_ratio: 0.0,
        product_authority_ratio: 0.0,
        corpus_health: CorpusHealthDiagnostic {
            kind: CorpusHealthKind::NoRetrievalCandidates,
            reason: "answered_from_follow_up_history".to_string(),
            selected_domain_count: 0,
            top_domain_count: 0,
        },
        full_doc_fetch_skipped: true,
        full_doc_fetch_skip_reason: "history_only",
        full_doc_fetch_errors: Vec::new(),
        detected_complexity: "simple",
        resolved_full_docs: 0,
        full_docs_source: "history_only",
        warnings: Vec::new(),
        explain: None,
    }
}

/// Back-compat: legacy timing shape always present; sub-stage fields populate
/// only when `cfg.ask_diagnostics` is true.
fn build_timing_json(
    retrieval_ms: u128,
    context_ms: u128,
    llm_ms: u128,
    total_ms: u128,
    timing: &AskTiming,
) -> serde_json::Value {
    fn ms(v: u128) -> serde_json::Value {
        serde_json::Value::Number(serde_json::Number::from(
            u64::try_from(v).unwrap_or(u64::MAX),
        ))
    }
    let mut obj = serde_json::Map::new();
    obj.insert("retrieval".into(), ms(retrieval_ms));
    obj.insert("context_build".into(), ms(context_ms));
    obj.insert("llm".into(), ms(llm_ms));
    obj.insert("total".into(), ms(total_ms));

    // Always emit streamed + ttft even without diagnostics.
    if let AskTiming::Disabled {
        streamed,
        llm_ttft_ms,
        ..
    } = timing
    {
        if let Some(v) = streamed {
            obj.insert("streamed".into(), (*v).into());
        }
        if let Some(v) = llm_ttft_ms {
            obj.insert("llm_ttft_ms".into(), ms(*v));
        }
        return serde_json::Value::Object(obj);
    }
    let Some(e) = timing.enabled() else {
        return serde_json::Value::Object(obj);
    };
    let timing_fields: &[(&str, Option<u128>)] = &[
        ("tei_embed_ms", e.tei_embed_ms),
        ("qdrant_primary_ms", e.qdrant_primary_ms),
        ("qdrant_secondary_ms", e.qdrant_secondary_ms),
        ("rerank_ms", e.rerank_ms),
        ("top_select_ms", e.top_select_ms),
        ("full_doc_fetch_ms", e.full_doc_fetch_ms),
        ("supplemental_ms", e.supplemental_ms),
        ("llm_ttft_ms", e.llm_ttft_ms),
        ("llm_total_ms", e.llm_total_ms),
        ("normalize_ms", e.normalize_ms),
    ];
    for &(key, val) in timing_fields {
        if let Some(v) = val {
            obj.insert(key.into(), ms(v));
        }
    }
    if let Some(v) = e.streamed {
        obj.insert("streamed".into(), v.into());
    }
    serde_json::Value::Object(obj)
}
