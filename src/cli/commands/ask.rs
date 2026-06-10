use crate::cli::commands::resolve_input_text;
use crate::core::config::Config;
use crate::core::error::diagnostics_from_error;
use crate::core::logging::{log_info, log_warn};
use crate::core::ui::{muted, primary};
use crate::services::events::ServiceEvent;
use crate::services::query as query_svc;
use crate::services::types::AskResult;
use std::error::Error;
use std::io::Write;
use std::time::Duration;
use tokio::sync::mpsc;

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
        "command=ask query_len={} effective_query_len={} collection={} follow_up={} session={}",
        query.len(),
        effective_query.len(),
        session_cfg.collection,
        session_cfg.ask_follow_up,
        active_session,
    ));

    if ask_cfg.ask_stream && !ask_cfg.json_output && !ask_cfg.ask_explain {
        println!("{}", primary("Conversation"));
        println!("  {} {}", primary("You:"), &query);
        println!("  {}", primary("Assistant:"));
    }

    let mut result = run_in_process_ask(&ask_cfg, &effective_query).await?;
    result.query = query.clone();
    result.session = Some(active_session.clone());

    if session_cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
        record_successful_turn(&session_cfg, &query, &result);
        return Ok(());
    }

    print_ask_human(&session_cfg, &query, &active_session, &result);

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

pub(crate) fn print_ask_human(cfg: &Config, query: &str, active_session: &str, result: &AskResult) {
    if cfg.ask_explain {
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
        print_ask_warnings(result);
        return;
    }

    if cfg.ask_stream {
        // Tokens were already streamed to stdout by the consumer; skip re-printing.
    } else {
        println!("{}", primary("Conversation"));
        println!("  {} {}", primary("You:"), query);
        println!("  {}", primary("Assistant:"));
        println!("{}", result.answer);
    }

    let stream_label = match result.timing_ms.streamed {
        Some(true) => " | streamed=yes",
        Some(false) => " | streamed=no",
        None => "",
    };
    let ttft_label = result
        .timing_ms
        .llm_ttft_ms
        .map(|ms| format!(" | ttft={ms}ms"))
        .unwrap_or_default();
    println!(
        "  {} retrieval={}ms | context={}ms | llm={}ms | total={}ms{ttft_label}{stream_label}",
        muted("Timing:"),
        result.timing_ms.retrieval,
        result.timing_ms.context_build,
        result.timing_ms.llm,
        result.timing_ms.total,
    );
    println!("  {} {}", muted("Session:"), active_session);
    print_ask_warnings(result);

    if cfg.ask_diagnostics {
        print_diagnostics(&result.diagnostics);
    }
}

fn print_ask_warnings(result: &AskResult) {
    if !result.warnings.is_empty() {
        println!("  {} {}", muted("Warnings:"), result.warnings.join(" | "));
    }
}

/// Timeout for draining the streaming consumer after the ask call returns.
const ASK_CONSUMER_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

async fn run_in_process_ask(cfg: &Config, query: &str) -> Result<AskResult, Box<dyn Error>> {
    let (event_tx, event_rx) = if cfg.ask_stream && !cfg.json_output {
        let (tx, rx) = mpsc::channel::<ServiceEvent>(256);
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    let mut consumer = event_rx.map(|mut rx| {
        tokio::spawn(async move {
            let mut stdout = std::io::stdout();
            let mut started = false;
            while let Some(event) = rx.recv().await {
                if let ServiceEvent::SynthesisDelta { text } = event {
                    if !started {
                        started = true;
                    }
                    let _ = stdout.write_all(text.as_bytes());
                    let _ = stdout.flush();
                }
            }
            if started {
                let _ = writeln!(stdout);
            }
        })
    });

    let result = match query_svc::ask(cfg, query, event_tx).await {
        Ok(result) => Ok(result),
        Err(err) => {
            if cfg.ask_diagnostics
                && let Some(diag) = diagnostics_from_error(err.as_ref())
            {
                eprintln!("{} {}", muted("Diagnostics:"), diag);
            }
            Err(err)
        }
    };

    if let Some(ref mut task) = consumer
        && tokio::time::timeout(ASK_CONSUMER_DRAIN_TIMEOUT, &mut *task)
            .await
            .is_err()
    {
        task.abort();
        log_warn("ask synthesis consumer timed out");
    }

    result
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
