//! Ask synthesis pipeline: turns an already-built [`super::AskContext`] into a
//! typed [`AskResult`] by calling the configured LLM backend, validating +
//! repairing citations, and assembling the result.
//!
//! Ported from legacy `axon_vector::ops::commands::ask` (the top-level
//! `ask_result_from_context(_with_deltas)` entry points and their private
//! helpers). The full legacy pipeline's `ask_result`/`ask_result_with_deltas`
//! (which built their own [`super::AskContext`]-equivalent via the legacy
//! `build_ask_context` reranker) are NOT ported here — `ask --explain` now
//! runs an entirely different, LLM-free path (see
//! `super::assemble::assemble_explain_result` and
//! `super::super::ask_retrieval::explain`). Two follow-on effects of that
//! split:
//!
//! - `can_answer_from_follow_up_history`/`history_only_ask_context` (legacy
//!   error-message-sniffing fallback for pure follow-up-history answers) are
//!   dropped: they only fired on error strings the legacy reranker raised
//!   (`"No candidates passed topical overlap"` /
//!   `"Failed to retrieve any context sources for ask"`), which
//!   `retrieval_ask_context`'s errors never produce, so the branch was already
//!   unreachable from the `axon-retrieval`-cutover `ask` path before this port.
//! - The `cfg.ask_explain` branch inside the legacy
//!   `synthesize_ask_from_context` (returning `build_explain_result`) never
//!   reaches this file at all: [`super::AskContext::from_retrieval`] is the
//!   only constructor used here, and the caller
//!   (`super::super::ask_retrieval::ask_via_retrieval`) short-circuits on
//!   `cfg.ask_explain` before this pipeline runs, calling
//!   `assemble_explain_result` directly instead.

use axon_api::AskResult;
use axon_core::config::Config;
use axon_core::logging::{log_info, log_warn};
use axon_llm as llm;
use std::collections::{BTreeMap, BTreeSet};

use super::AskContext;
use super::assemble::assemble_ask_result;
use super::normalize::{self, normalize_ask_answer, summarize_citation_validation};
use super::output::{AskLlmCompletion, ask_llm_answer, ask_llm_answer_with_deltas};
use super::timing::{AskTiming, AskTimingSlot};

pub(crate) fn validate_ask_llm_config(cfg: &Config) -> anyhow::Result<()> {
    let backend = llm::LlmBackendConfig::from_config(cfg);
    match backend.kind {
        llm::LlmBackendKind::GeminiHeadless => {
            llm::runtime::headless::gemini::validate_config(&backend)
                .map_err(|e| anyhow::anyhow!("{e}"))
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
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "AXON_SYNTHESIS_OPENAI_MODEL is required for ask (legacy alias: AXON_OPENAI_MODEL)"
                    )
            })?;
            Ok(())
        }
        llm::LlmBackendKind::CodexAppServer => {
            llm::runtime::codex_app_server::validate_config(&backend)
                .map_err(|e| anyhow::anyhow!("{e}"))
        }
    }
}

/// Synthesize an `ask` answer from a **prebuilt** [`AskContext`] (no streaming).
#[must_use = "ask_result_from_context returns a Result that should be handled"]
pub async fn ask_result_from_context(
    cfg: &Config,
    query: &str,
    ctx: AskContext,
    ask_started: std::time::Instant,
) -> anyhow::Result<AskResult> {
    let diagnostics_enabled = cfg.ask_diagnostics || cfg.ask_explain;
    let timing = AskTiming::new(diagnostics_enabled, ask_started);
    synthesize_ask_from_context(
        cfg,
        query,
        ctx,
        Option::<fn(&str)>::None,
        timing,
        ask_started,
    )
    .await
}

/// Streaming variant of [`ask_result_from_context`]: forwards synthesis token
/// deltas to `on_delta` as the LLM streams.
#[must_use = "ask_result_from_context_with_deltas returns a Result that should be handled"]
pub async fn ask_result_from_context_with_deltas<F>(
    cfg: &Config,
    query: &str,
    ctx: AskContext,
    ask_started: std::time::Instant,
    on_delta: F,
) -> anyhow::Result<AskResult>
where
    F: FnMut(&str) + Send,
{
    let diagnostics_enabled = cfg.ask_diagnostics || cfg.ask_explain;
    let timing = AskTiming::new(diagnostics_enabled, ask_started);
    synthesize_ask_from_context(cfg, query, ctx, Some(on_delta), timing, ask_started).await
}

/// Run the synthesis half of `ask` over an already-built [`AskContext`].
async fn synthesize_ask_from_context<F>(
    cfg: &Config,
    query: &str,
    ctx: AskContext,
    mut on_delta: Option<F>,
    mut timing: AskTiming,
    ask_started: std::time::Instant,
) -> anyhow::Result<AskResult>
where
    F: FnMut(&str) + Send,
{
    let diagnostics_enabled = cfg.ask_diagnostics || cfg.ask_explain;
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

    let context = ask_context_with_follow_up(cfg, &ctx.context);
    let (answer, llm_total_ms) =
        resolve_answer_and_timing(cfg, query, &context, on_delta.take(), &mut timing).await?;
    Ok(assemble_ask_result(
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

/// Run the LLM step and normalise the answer text. Returns the normalised
/// answer and the raw LLM wall-clock ms.
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
        ask_llm_answer_with_deltas(cfg, query, context, callback).await
    } else {
        ask_llm_answer(cfg, query, context).await
    }
    .map_err(|e| anyhow::anyhow!("LLM answer generation failed: {e}"))?;

    let (answer_text, mut llm_total_ms) = match &llm {
        AskLlmCompletion::Streamed {
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
        AskLlmCompletion::Fallback {
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
    let repair_query = build_citation_repair_query(query, context, invalid_answer, validation);
    let repaired = ask_llm_answer(&repair_cfg, &repair_query, context)
        .await
        .map_err(|e| anyhow::anyhow!("citation repair retry failed: {e}"))?;
    let repaired_answer_text = match repaired {
        AskLlmCompletion::Streamed { answer, .. } | AskLlmCompletion::Fallback { answer, .. } => {
            answer
        }
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

fn build_citation_repair_query(
    query: &str,
    context: &str,
    invalid_answer: &str,
    validation: &normalize::CitationValidationSummary,
) -> String {
    let source_map = normalize::parse_context_source_map(context);
    let mut group_index_by_identity: BTreeMap<String, usize> = BTreeMap::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut group_by_source_id: BTreeMap<usize, usize> = BTreeMap::new();
    for (source_id, source) in &source_map {
        let identity = normalize::canonical_source_identity(source);
        let group_index = match group_index_by_identity.get(&identity).copied() {
            Some(index) => index,
            None => {
                let index = groups.len();
                groups.push(Vec::new());
                group_index_by_identity.insert(identity, index);
                index
            }
        };
        groups[group_index].push(*source_id);
        group_by_source_id.insert(*source_id, group_index);
    }

    let group_lines = groups
        .iter()
        .enumerate()
        .map(|(index, source_ids)| {
            let citations = source_ids
                .iter()
                .map(|id| format!("[S{id}]"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("- Document group D{}: {citations}", index + 1)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let cited_groups = normalize::extract_cited_source_ids(invalid_answer)
        .into_iter()
        .filter_map(|source_id| group_by_source_id.get(&source_id).copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|index| format!("D{}", index + 1))
        .collect::<Vec<_>>();
    let cited_group_summary = if cited_groups.is_empty() {
        "none".to_string()
    } else {
        cited_groups.join(", ")
    };

    format!(
        "{query}\n\nYour previous answer failed Axon citation validation:\n{}\n\n\
Retrieved source-document groups (derived from the same canonical identities used by validation):\n{}\n\
The previous answer cited document groups: {cited_group_summary}.\n\n\
Rewrite the answer from scratch using the retrieved context. Citations within one document group do not count as distinct sources. For a non-trivial answer, cite evidence from at least two different document groups when two or more relevant groups are available. Every factual claim must remain grounded in its supporting [S#] citation. Never attach a citation merely to satisfy the count; if fewer than two groups contain relevant support, state that the indexed evidence is insufficient.\n\n\
Previous invalid answer:\n{}",
        validation
            .issues
            .iter()
            .map(|issue| format!("- {issue}"))
            .collect::<Vec<_>>()
            .join("\n"),
        if group_lines.is_empty() {
            "- No mapped source documents.".to_string()
        } else {
            group_lines
        },
        invalid_answer.trim()
    )
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

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
