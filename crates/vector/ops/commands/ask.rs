use anyhow::Context;

use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;

mod context;
mod normalize;
mod output;
#[cfg(test)]
mod tests;

pub(crate) use context::{AskContext, build_ask_context};

pub(super) fn validate_ask_llm_config(cfg: &Config) -> anyhow::Result<()> {
    anyhow::ensure!(
        !cfg.openai_base_url.trim().is_empty() && !cfg.openai_model.trim().is_empty(),
        "OPENAI_BASE_URL and OPENAI_MODEL are required for ask/evaluate commands"
    );
    Ok(())
}

pub async fn ask_payload(cfg: &Config, query: &str) -> anyhow::Result<serde_json::Value> {
    let ask_started = std::time::Instant::now();

    log_info(&format!(
        "ask query_len={} collection={}",
        query.len(),
        cfg.collection
    ));
    validate_ask_llm_config(cfg)?;

    let ctx = build_ask_context(cfg, query)
        .await
        .context("failed to build ask context")?;
    let (raw_answer, llm_elapsed_ms, _) = output::ask_llm_answer(cfg, query, &ctx.context)
        .await
        .map_err(|e| anyhow::anyhow!("LLM answer generation failed: {e}"))?;
    let answer = normalize::normalize_ask_answer(cfg, query, &raw_answer, &ctx.context);
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
                "dropped_by_allowlist": ctx.dropped_by_allowlist,
            })
        } else {
            serde_json::Value::Null
        },
        "timing_ms": {
            "retrieval": ctx.retrieval_elapsed_ms,
            "context_build": ctx.context_elapsed_ms,
            "graph": ctx.graph_elapsed_ms,
            "llm": llm_elapsed_ms,
            "total": total_elapsed_ms,
        }
    }))
}
