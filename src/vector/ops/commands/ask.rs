use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::services::llm_backend;
use crate::services::types::{CorpusHealthDiagnostic, CorpusHealthKind};

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
pub(crate) use normalize::normalize_ask_answer;
pub(crate) use timing::{AskTiming, AskTimingSlot};

pub(super) fn validate_ask_llm_config(cfg: &Config) -> anyhow::Result<()> {
    let backend = llm_backend::LlmBackendConfig::from_config(cfg);
    match backend.kind {
        llm_backend::LlmBackendKind::GeminiHeadless => {
            llm_backend::headless::gemini::validate_config(&backend)
                .map_err(|e| anyhow::anyhow!("{e}"))
        }
        llm_backend::LlmBackendKind::OpenAiCompat => {
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
        let total_elapsed_ms = ask_started.elapsed().as_millis();
        log_info(&format!(
            "ask explain complete total_ms={total_elapsed_ms} context_chars={}",
            ctx.context.len()
        ));
        return Ok(serde_json::json!({
            "query": query,
            "answer": "",
            "session": serde_json::Value::Null,
            "diagnostics": build_diagnostics_json(diagnostics_enabled, cfg, &ctx),
            "explain": ctx.explain,
            "timing_ms": build_timing_json(
                ctx.retrieval_elapsed_ms,
                ctx.context_elapsed_ms,
                0,
                total_elapsed_ms,
                &timing,
            ),
        }));
    }

    validate_ask_llm_config(cfg)?;
    let context = ask_context_with_follow_up(cfg, &ctx.context);
    log_info(&format!(
        "ask llm starting backend={:?} model={} context_chars={} stream={}",
        cfg.llm_backend,
        llm_backend::configured_model_from_config(cfg).unwrap_or_else(|| "<default>".to_string()),
        context.len(),
        cfg.ask_stream,
    ));
    let llm = output::ask_llm_answer(cfg, query, &context)
        .await
        .map_err(|e| anyhow::anyhow!("LLM answer generation failed: {e}"))?;
    let (answer_text, llm_total_ms) = match &llm {
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
    let answer = normalize_ask_answer(cfg, query, answer_text, &context);
    timing.record(AskTimingSlot::Normalize, normalize_started);
    if cfg.ask_stream && !cfg.json_output && !cfg.ask_explain && answer.trim() != answer_text.trim()
    {
        print_normalized_stream_correction(&answer);
    }

    let total_elapsed_ms = ask_started.elapsed().as_millis();
    log_info(&format!(
        "ask complete answer_chars={} llm_ms={} total_ms={}",
        answer.len(),
        llm_total_ms,
        total_elapsed_ms,
    ));

    Ok(serde_json::json!({
        "query": query,
        "answer": answer,
        "session": serde_json::Value::Null,
        "diagnostics": build_diagnostics_json(diagnostics_enabled, cfg, &ctx),
        "explain": serde_json::Value::Null,
        "timing_ms": build_timing_json(
            ctx.retrieval_elapsed_ms,
            ctx.context_elapsed_ms,
            llm_total_ms,
            total_elapsed_ms,
            &timing,
        ),
    }))
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
        detected_complexity: "simple",
        resolved_full_docs: 0,
        full_docs_source: "history_only",
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

    let Some(e) = timing.enabled() else {
        return serde_json::Value::Object(obj);
    };
    if let Some(v) = e.tei_embed_ms {
        obj.insert("tei_embed_ms".into(), ms(v));
    }
    if let Some(v) = e.qdrant_primary_ms {
        obj.insert("qdrant_primary_ms".into(), ms(v));
    }
    if let Some(v) = e.qdrant_secondary_ms {
        obj.insert("qdrant_secondary_ms".into(), ms(v));
    }
    if let Some(v) = e.rerank_ms {
        obj.insert("rerank_ms".into(), ms(v));
    }
    if let Some(v) = e.top_select_ms {
        obj.insert("top_select_ms".into(), ms(v));
    }
    if let Some(v) = e.full_doc_fetch_ms {
        obj.insert("full_doc_fetch_ms".into(), ms(v));
    }
    if let Some(v) = e.supplemental_ms {
        obj.insert("supplemental_ms".into(), ms(v));
    }
    if let Some(v) = e.llm_ttft_ms {
        obj.insert("llm_ttft_ms".into(), ms(v));
    }
    if let Some(v) = e.llm_total_ms {
        obj.insert("llm_total_ms".into(), ms(v));
    }
    if let Some(v) = e.streamed {
        obj.insert("streamed".into(), v.into());
    }
    if let Some(v) = e.normalize_ms {
        obj.insert("normalize_ms".into(), ms(v));
    }
    serde_json::Value::Object(obj)
}
