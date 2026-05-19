use crate::cli::client::ServerClient;
use crate::cli::commands::resolve_input_text;
use crate::core::config::Config;
use crate::core::logging::{log_info, log_warn};
use crate::core::ui::{muted, primary};
use crate::services::error::diagnostics_from_error;
use crate::services::query as query_svc;
use crate::services::types::AskResult;
use std::error::Error;

mod followup;

pub async fn run_ask(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if cfg.ask_list_sessions {
        return run_list_sessions(cfg);
    }

    let query = resolve_input_text(cfg).ok_or("ask requires a question")?;
    let (session_cfg, active_session) = prepare_ask_session(cfg)?;
    let effective_query = if session_cfg.ask_follow_up {
        followup::follow_up_query(&session_cfg, &query)?.unwrap_or_else(|| query.clone())
    } else {
        query.clone()
    };
    let mut ask_cfg = session_cfg.clone();
    if session_cfg.ask_follow_up {
        ask_cfg.ask_follow_up_context = followup::follow_up_context_source(&session_cfg)?;
    }
    log_info(&format!(
        "command=ask query_len={} effective_query_len={} collection={} follow_up={} session={} server_url={}",
        query.len(),
        effective_query.len(),
        session_cfg.collection,
        session_cfg.ask_follow_up,
        active_session,
        session_cfg
            .server_url
            .as_ref()
            .map(|u| u.as_str())
            .unwrap_or("(in-process)")
    ));

    if ask_cfg.ask_stream && !ask_cfg.json_output && !ask_cfg.ask_explain {
        println!("{}", primary("Conversation"));
        println!("  {} {}", primary("You:"), query);
        println!("  {}", primary("Assistant:"));
    }

    let mut result =
        if (ask_cfg.ask_stream || ask_cfg.ask_follow_up) && ask_cfg.server_url.is_some() {
            run_in_process_ask(&ask_cfg, &effective_query).await?
        } else if let Some(server_url) = ask_cfg.server_url.as_ref() {
            match ask_via_server(&ask_cfg, server_url, &effective_query).await {
                Ok(result) => result,
                Err(err) => {
                    let msg = err.to_string();
                    let hint = hint_for_ask_error(&msg);
                    eprintln!(
                        "{} ask failed via server-url '{server_url}': {err}",
                        muted("Error:")
                    );
                    if let Some(h) = hint {
                        eprintln!("  hint: {h}");
                    }
                    return Err(err);
                }
            }
        } else {
            run_in_process_ask(&ask_cfg, &effective_query).await?
        };
    result.query = query.clone();
    result.session = Some(active_session.clone());

    if session_cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
        record_successful_turn(&session_cfg, &query, &result);
        return Ok(());
    }

    if session_cfg.ask_explain {
        println!("{}", primary("Ask Explain"));
        println!("  {} {}", primary("Query:"), query);
        println!("  {} {}", muted("Session:"), active_session);
        println!(
            "  {} reranked={} context_sources={} llm_skipped=true",
            muted("Trace:"),
            result
                .explain
                .as_ref()
                .map(|e| e.candidates.len())
                .unwrap_or(0),
            result
                .explain
                .as_ref()
                .map(|e| e.context.final_source_order.len())
                .unwrap_or(0),
        );
        println!(
            "  {} rerun with --json for the full explain trace",
            muted("Hint:")
        );
        return Ok(());
    }

    if session_cfg.ask_stream {
        println!();
    } else {
        println!("{}", primary("Conversation"));
        println!("  {} {}", primary("You:"), query);
        println!("  {}", primary("Assistant:"));
        println!("{}", result.answer);
    }

    println!(
        "  {} retrieval={}ms | context={}ms | llm={}ms | total={}ms",
        muted("Timing:"),
        result.timing_ms.retrieval,
        result.timing_ms.context_build,
        result.timing_ms.llm,
        result.timing_ms.total,
    );
    println!("  {} {}", muted("Session:"), active_session);

    if session_cfg.ask_diagnostics {
        print_diagnostics(&result.diagnostics);
    }

    record_successful_turn(&session_cfg, &query, &result);

    Ok(())
}

/// Resolve the target ask session, wipe history if `--new-session` or
/// `--reset-session` requested, and return the prepared session config.
fn prepare_ask_session(cfg: &Config) -> Result<(Config, String), Box<dyn Error>> {
    // `--new-session`: pick an explicit `--session NAME` or auto-generate one,
    // delete any prior history, and treat this as a fresh thread (no follow-up
    // context). clap enforces exclusivity with `--follow-up`, `--resume`, and
    // `--reset-session`.
    let new_session_name = cfg.ask_new_session.then(|| {
        cfg.ask_session
            .clone()
            .unwrap_or_else(followup::new_session_name)
    });

    let active_session = match new_session_name.as_ref() {
        Some(name) => name.clone(),
        None => followup::resolve_selected_session_name(cfg)?,
    };
    let mut session_cfg = cfg.clone();
    session_cfg.ask_session = Some(active_session.clone());

    if new_session_name.is_some() {
        followup::reset_session(&session_cfg)?;
        session_cfg.ask_follow_up = false;
    } else if session_cfg.ask_reset_session {
        followup::reset_session(&session_cfg)?;
    }
    Ok((session_cfg, active_session))
}

fn run_list_sessions(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let sessions = followup::list_sessions()?;

    if cfg.json_output {
        let payload: Vec<serde_json::Value> = sessions
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "turn_count": s.turn_count,
                    "last_used_unix": s.last_used_unix,
                    "is_latest": s.is_latest,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if sessions.is_empty() {
        println!("{} no local ask sessions yet", muted("Sessions:"));
        println!(
            "  {} run `axon ask \"...\"` to create one (saved to ~/.axon/ask-sessions/)",
            muted("Hint:")
        );
        return Ok(());
    }

    let now = chrono::Utc::now().timestamp();
    println!(
        "{:<32}  {:>5}  {:<32}  LATEST",
        "NAME", "TURNS", "LAST USED"
    );
    for s in &sessions {
        let last = match s.last_used_unix {
            Some(ts) => {
                let abs = chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                    .unwrap_or_else(|| "-".to_string());
                let rel = format_relative_secs(now.saturating_sub(ts));
                if rel.is_empty() {
                    abs
                } else {
                    format!("{abs} ({rel})")
                }
            }
            None => "-".to_string(),
        };
        let star = if s.is_latest { "*" } else { "" };
        println!(
            "{:<32}  {:>5}  {:<32}  {}",
            s.name, s.turn_count, last, star
        );
    }
    Ok(())
}

fn format_relative_secs(secs: i64) -> String {
    if secs < 0 {
        return String::new();
    }
    let secs = secs as u64;
    if secs < 60 {
        return format!("{secs}s ago");
    }
    let minutes = secs / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    let days = hours / 24;
    format!("{days}d ago")
}

fn record_successful_turn(cfg: &Config, query: &str, result: &AskResult) {
    if cfg.ask_explain || result.answer.trim().is_empty() {
        return;
    }
    if let Err(err) = followup::append_turn(cfg, query, &result.answer) {
        log_warn(&format!(
            "ask: failed to record follow-up session turn: {err}"
        ));
    }
    if let Err(err) = followup::update_latest_session(cfg) {
        log_warn(&format!("ask: failed to update latest ask session: {err}"));
    }
}

async fn run_in_process_ask(cfg: &Config, query: &str) -> Result<AskResult, Box<dyn Error>> {
    match query_svc::ask(cfg, query, None).await {
        Ok(result) => Ok(result),
        Err(err) => {
            if cfg.ask_diagnostics
                && let Some(diag) = diagnostics_from_error(err.as_ref())
            {
                eprintln!("{} {}", muted("Diagnostics:"), diag);
            }
            Err(err)
        }
    }
}

/// Map an `ask_via_server` error message prefix to a short user hint.
///
/// Returns `None` when the error class doesn't have a useful, non-noisy hint
/// (e.g. generic 4xx client errors). Kept as a pure function so unit tests
/// can cover the full match without parsing stderr.
pub(crate) fn hint_for_ask_error(msg: &str) -> Option<&'static str> {
    if msg.starts_with("connect to ") {
        return Some(
            "ensure `axon serve` is running there, or unset AXON_SERVER_URL to fall back to in-process ask.",
        );
    }
    if msg.starts_with("server returned 401") || msg.starts_with("server returned 403") {
        return Some("AXON_MCP_HTTP_TOKEN does not match the server's token.");
    }
    if msg.starts_with("server returned 4") {
        return None;
    }
    if msg.starts_with("decode AskResult") {
        return Some(
            "server response did not match expected schema; check axon serve version compatibility.",
        );
    }
    if msg.starts_with("refusing to send AXON_MCP_HTTP_TOKEN") {
        return Some("set AXON_SERVER_INSECURE=1 to override, or use https / a loopback host.");
    }
    None
}

/// POST the ask request to a running `axon serve` instance and deserialize the
/// `AskResult` response from a running server.
pub(crate) async fn ask_via_server(
    cfg: &Config,
    server_url: &reqwest::Url,
    query: &str,
) -> Result<AskResult, Box<dyn Error>> {
    let endpoint = format!("{}/v1/ask", server_url.as_str().trim_end_matches('/'));
    let mut payload = serde_json::Map::new();
    payload.insert("query".into(), serde_json::Value::String(query.to_string()));
    payload.insert(
        "collection".into(),
        serde_json::Value::String(cfg.collection.clone()),
    );
    if let Some(ref s) = cfg.since {
        payload.insert("since".into(), serde_json::Value::String(s.clone()));
    }
    if let Some(ref b) = cfg.before {
        payload.insert("before".into(), serde_json::Value::String(b.clone()));
    }
    payload.insert(
        "diagnostics".into(),
        serde_json::Value::Bool(cfg.ask_diagnostics || cfg.ask_explain),
    );
    payload.insert("explain".into(), serde_json::Value::Bool(cfg.ask_explain));
    payload.insert(
        "hybrid_search".into(),
        serde_json::Value::Bool(cfg.hybrid_search_enabled),
    );
    payload.insert(
        "ask_chunk_limit".into(),
        serde_json::Value::from(cfg.ask_chunk_limit),
    );
    payload.insert(
        "ask_full_docs".into(),
        serde_json::Value::from(cfg.ask_full_docs),
    );
    payload.insert(
        "ask_max_context_chars".into(),
        serde_json::Value::from(cfg.ask_max_context_chars),
    );
    payload.insert(
        "ask_hybrid_candidates".into(),
        serde_json::Value::from(cfg.ask_hybrid_candidates),
    );
    payload.insert(
        "ask_min_relevance_score".into(),
        serde_json::Value::from(cfg.ask_min_relevance_score),
    );
    payload.insert(
        "ask_doc_chunk_limit".into(),
        serde_json::Value::from(cfg.ask_doc_chunk_limit),
    );
    payload.insert(
        "ask_doc_fetch_concurrency".into(),
        serde_json::Value::from(cfg.ask_doc_fetch_concurrency),
    );
    payload.insert(
        "ask_backfill_chunks".into(),
        serde_json::Value::from(cfg.ask_backfill_chunks),
    );
    payload.insert(
        "ask_candidate_limit".into(),
        serde_json::Value::from(cfg.ask_candidate_limit),
    );
    payload.insert(
        "ask_min_citations_nontrivial".into(),
        serde_json::Value::from(cfg.ask_min_citations_nontrivial),
    );
    payload.insert(
        "ask_authoritative_domains".into(),
        serde_json::to_value(&cfg.ask_authoritative_domains)?,
    );
    payload.insert(
        "ask_authoritative_boost".into(),
        serde_json::Value::from(cfg.ask_authoritative_boost),
    );

    let client =
        ServerClient::new(server_url.clone()).map_err(|e| -> Box<dyn Error> { e.into() })?;
    let result: AskResult = client
        .post_json(&endpoint_path(server_url, &endpoint), &payload, "AskResult")
        .await
        .map_err(|e| -> Box<dyn Error> { e.into() })?;
    Ok(result)
}

fn endpoint_path(server_url: &reqwest::Url, endpoint: &str) -> String {
    endpoint
        .trim_start_matches(server_url.as_str().trim_end_matches('/'))
        .trim_start_matches('/')
        .to_string()
}

fn print_diagnostics(diag: &Option<crate::services::types::AskDiagnostics>) {
    let Some(diag) = diag else {
        return;
    };

    println!(
        "  {} candidates={} reranked={} chunks={} full_docs={} supplemental={} context_chars={} authority_ratio={:.2} configured_authority={:.2} product_authority={:.2}",
        muted("Diagnostics:"),
        diag.candidate_pool,
        diag.reranked_pool,
        diag.chunks_selected,
        diag.full_docs_selected,
        diag.supplemental_selected,
        diag.context_chars,
        diag.authority_ratio,
        diag.configured_authority_ratio,
        diag.product_authority_ratio,
    );

    if !diag.top_domains.is_empty() {
        println!(
            "  {} {}",
            muted("Top domains:"),
            diag.top_domains.join(", ")
        );
    }
    if let Some(health) = &diag.corpus_health {
        println!(
            "  {} {:?} selected_domains={} top_domains={} reason={}",
            muted("Corpus health:"),
            health.kind,
            health.selected_domain_count,
            health.top_domain_count,
            health.reason
        );
    }
}

#[cfg(test)]
mod ask_via_server_tests;
