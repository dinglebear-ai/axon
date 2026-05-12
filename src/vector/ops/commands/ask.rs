use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::services::llm_backend;

mod context;
mod normalize;
mod output;
pub(super) mod synthesis_prompt;
#[cfg(test)]
mod tests;
pub(crate) mod timing;

pub(crate) use context::{AskContext, build_ask_context};
pub(crate) use normalize::normalize_ask_answer;
pub(crate) use timing::{AskTiming, AskTimingSlot};

pub(super) fn validate_ask_llm_config(cfg: &Config) -> anyhow::Result<()> {
    let _ = cfg;
    let backend = llm_backend::LlmBackendConfig::from_config(cfg);
    llm_backend::headless::gemini::validate_config(&backend).map_err(|e| anyhow::anyhow!("{e}"))
}

pub async fn ask_payload(cfg: &Config, query: &str) -> anyhow::Result<serde_json::Value> {
    let ask_started = std::time::Instant::now();
    let mut timing = AskTiming::new(cfg.ask_diagnostics, ask_started);

    log_info(&format!(
        "ask query_len={} collection={}",
        query.len(),
        cfg.collection
    ));
    validate_ask_llm_config(cfg)?;

    let ctx = build_ask_context(cfg, query, &mut timing).await?;
    let llm = output::ask_llm_answer(cfg, query, &ctx.context)
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
    let answer = normalize_ask_answer(cfg, query, answer_text, &ctx.context);
    timing.record(AskTimingSlot::Normalize, normalize_started);

    let total_elapsed_ms = ask_started.elapsed().as_millis();

    Ok(serde_json::json!({
        "query": query,
        "answer": answer,
        "diagnostics": if cfg.ask_diagnostics {
            serde_json::json!({
                "candidate_pool": ctx.candidate_count,
                "reranked_pool": ctx.reranked_count,
                "chunks_selected": ctx.chunks_selected,
                "full_docs_selected": ctx.full_docs_selected,
                "supplemental_selected": ctx.supplemental_count,
                "context_chars": ctx.context.len(),
                "graph_entities": ctx.graph_entities_found,
                "graph_context_chars": ctx.graph_context_text.len(),
                "min_relevance_score": cfg.ask_min_relevance_score,
                "doc_fetch_concurrency": cfg.ask_doc_fetch_concurrency,
                "top_domains": ctx.top_domains,
                "authority_ratio": ctx.authoritative_ratio,
                "full_doc_fetch_skipped": ctx.full_doc_fetch_skipped,
                "full_doc_fetch_skip_reason": ctx.full_doc_fetch_skip_reason,
                "detected_complexity": ctx.detected_complexity,
                "resolved_full_docs": ctx.resolved_full_docs,
                "full_docs_source": ctx.full_docs_source,
            })
        } else {
            serde_json::Value::Null
        },
        "timing_ms": build_timing_json(
            ctx.retrieval_elapsed_ms,
            ctx.context_elapsed_ms,
            ctx.graph_elapsed_ms,
            llm_total_ms,
            total_elapsed_ms,
            &timing,
        ),
    }))
}

/// Back-compat: legacy 5-bucket shape always present; sub-stage fields populate
/// only when `cfg.ask_diagnostics` is true.
fn build_timing_json(
    retrieval_ms: u128,
    context_ms: u128,
    graph_ms: u128,
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
    obj.insert("graph".into(), ms(graph_ms));
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
