use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::services::acp_llm;

mod context;
mod normalize;
mod output;
#[cfg(test)]
mod tests;
pub(crate) mod timing;

pub(crate) use context::{AskContext, build_ask_context};
pub(crate) use normalize::normalize_ask_answer;
pub(crate) use timing::{AskTiming, AskTimingSlot};

pub(super) fn validate_ask_llm_config(cfg: &Config) -> anyhow::Result<()> {
    anyhow::ensure!(
        cfg.acp_adapter_cmd
            .as_deref()
            .is_some_and(|program| !program.trim().is_empty()),
        "ask/evaluate requires an ACP adapter — set AXON_ASK_AGENT=claude|codex|gemini \
         (uses the AXON_ACP_<AGENT>_ADAPTER_CMD you already have configured) \
         or set AXON_ACP_ADAPTER_CMD directly"
    );
    Ok(())
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

    // Start warming the ACP adapter before context retrieval so the cold-start
    // overlaps with Qdrant queries instead of running sequentially after them.
    let warm_started = std::time::Instant::now();
    let warm = match acp_llm::warm_session(cfg, None) {
        Ok(w) => {
            // The warm-pool checkout returns immediately on cache hit; cold spawns
            // also return quickly because adapter init is asynchronous (see
            // spawn_warm_session / spawn_eager). Either way, this slot reflects
            // the synchronous portion of warm-session acquisition.
            timing.set_warm_path(w.from_pool());
            Some(w)
        }
        Err(e) => {
            log_warn(&format!(
                "ask: warm session failed to start, using cold path: {e}"
            ));
            timing.set_warm_path(false);
            None
        }
    };
    timing.record(AskTimingSlot::WarmSessionReady, warm_started);

    let ctx = build_ask_context(cfg, query, &mut timing).await?;
    let llm = output::ask_llm_answer(cfg, query, &ctx.context, warm)
        .await
        .map_err(|e| anyhow::anyhow!("LLM answer generation failed: {e}"))?;
    timing.set(AskTimingSlot::LlmTotal, llm.llm_total_ms);
    if let Some(ttft_at) = llm.ttft_first_token_at {
        // TTFT is measured from request_start (= ask_started, the
        // outermost-observable entry point in the ask service). This includes
        // any ACP cold-start tax incurred during warm_session and retrieval.
        let ttft_ms = ttft_at
            .saturating_duration_since(timing.request_start)
            .as_millis();
        timing.set_ttft(ttft_ms);
    }

    let normalize_started = std::time::Instant::now();
    let answer = normalize_ask_answer(cfg, query, &llm.answer, &ctx.context);
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
            })
        } else {
            serde_json::Value::Null
        },
        "timing_ms": build_timing_json(
            ctx.retrieval_elapsed_ms,
            ctx.context_elapsed_ms,
            ctx.graph_elapsed_ms,
            llm.llm_total_ms,
            total_elapsed_ms,
            &timing,
        ),
    }))
}

/// Build the `timing_ms` JSON object. The legacy 5-bucket shape
/// (`retrieval` / `context_build` / `graph` / `llm` / `total`) is always
/// present for back-compat; sub-stage fields populate only when
/// `cfg.ask_diagnostics` is true (otherwise omitted via `skip_serializing_if`
/// at the typed boundary in `crates/services/types/service.rs`).
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

    if let Some(v) = timing.warm_session_ready_ms {
        obj.insert("warm_session_ready_ms".into(), ms(v));
    }
    if let Some(v) = timing.tei_embed_ms {
        obj.insert("tei_embed_ms".into(), ms(v));
    }
    if let Some(v) = timing.qdrant_primary_ms {
        obj.insert("qdrant_primary_ms".into(), ms(v));
    }
    if let Some(v) = timing.qdrant_secondary_ms {
        obj.insert("qdrant_secondary_ms".into(), ms(v));
    }
    if let Some(v) = timing.rerank_ms {
        obj.insert("rerank_ms".into(), ms(v));
    }
    if let Some(v) = timing.top_select_ms {
        obj.insert("top_select_ms".into(), ms(v));
    }
    if let Some(v) = timing.full_doc_fetch_ms {
        obj.insert("full_doc_fetch_ms".into(), ms(v));
    }
    if let Some(v) = timing.supplemental_ms {
        obj.insert("supplemental_ms".into(), ms(v));
    }
    if let Some(v) = timing.llm_ttft_ms {
        obj.insert("llm_ttft_ms".into(), ms(v));
    }
    if let Some(v) = timing.llm_total_ms {
        obj.insert("llm_total_ms".into(), ms(v));
    }
    if let Some(v) = timing.llm_warm_path {
        obj.insert("llm_warm_path".into(), v.into());
    }
    if let Some(v) = timing.normalize_ms {
        obj.insert("normalize_ms".into(), ms(v));
    }
    serde_json::Value::Object(obj)
}
