use crate::cli::commands::resolve_input_text;
use crate::core::config::Config;
use crate::core::http::build_client;
use crate::core::logging::log_info;
use crate::core::ui::{muted, primary};
use crate::services::error::diagnostics_from_error;
use crate::services::query as query_svc;
use crate::services::types::AskResult;
use std::error::Error;
use std::net::IpAddr;

const ASK_VIA_SERVER_TIMEOUT_SECS: u64 = 300;

pub async fn run_ask(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let query = resolve_input_text(cfg).ok_or("ask requires a question")?;
    log_info(&format!(
        "command=ask query_len={} collection={} server_url={}",
        query.len(),
        cfg.collection,
        cfg.server_url
            .as_ref()
            .map(|u| u.as_str())
            .unwrap_or("(in-process)")
    ));

    let result = if let Some(server_url) = cfg.server_url.as_ref() {
        match ask_via_server(cfg, server_url, &query).await {
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
        match query_svc::ask(cfg, &query, None).await {
            Ok(result) => result,
            Err(err) => {
                if cfg.ask_diagnostics
                    && let Some(diag) = diagnostics_from_error(err.as_ref())
                {
                    eprintln!("{} {}", muted("Diagnostics:"), diag);
                }
                return Err(err);
            }
        }
    };

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!("{}", primary("Conversation"));
    println!("  {} {}", primary("You:"), query);
    println!("  {} {}", primary("Assistant:"), result.answer);

    println!(
        "  {} retrieval={}ms | context={}ms | llm={}ms | total={}ms",
        muted("Timing:"),
        result.timing_ms.retrieval,
        result.timing_ms.context_build,
        result.timing_ms.llm,
        result.timing_ms.total,
    );

    if cfg.ask_diagnostics {
        print_diagnostics(&result.diagnostics);
    }

    Ok(())
}

/// Map an `ask_via_server` error message prefix to a short user hint.
///
/// Returns `None` when the error class doesn't have a useful, non-noisy hint
/// (e.g. generic 4xx client errors). Kept as a pure function so unit tests
/// can cover the full match without parsing stderr.
pub(crate) fn hint_for_ask_error(msg: &str) -> Option<&'static str> {
    if msg.starts_with("connect to ") {
        return Some(
            "ensure `axon serve` is running there, or unset --server-url / AXON_ASK_SERVER_URL to fall back to in-process ask.",
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
        return Some(
            "set AXON_ASK_INSECURE=1 to override (not recommended), or use https / a loopback host.",
        );
    }
    None
}

/// Returns true when `host_str` represents a loopback destination
/// (127.0.0.0/8, ::1, or the literal "localhost").
fn is_loopback_host(host_str: &str) -> bool {
    if host_str.eq_ignore_ascii_case("localhost") {
        return true;
    }
    // Strip optional bracketed IPv6 form ("[::1]") before parsing.
    let trimmed = host_str
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host_str);
    if let Ok(ip) = trimmed.parse::<IpAddr>() {
        return ip.is_loopback();
    }
    false
}

/// Returns Ok(()) if it is safe to attach a bearer token to a request to `url`.
/// Refuses cleartext-bearer over `http://` to non-loopback hosts unless the user
/// has set `AXON_ASK_INSECURE=1`.
pub(crate) fn check_cleartext_token_allowed(url: &reqwest::Url) -> Result<(), String> {
    if url.scheme() != "http" {
        return Ok(());
    }
    let host = url.host_str().unwrap_or("");
    if is_loopback_host(host) {
        return Ok(());
    }
    if std::env::var("AXON_ASK_INSECURE").ok().as_deref() == Some("1") {
        return Ok(());
    }
    Err(format!(
        "refusing to send AXON_MCP_HTTP_TOKEN over plaintext HTTP to non-loopback host '{host}'; set AXON_ASK_INSECURE=1 to override (not recommended)"
    ))
}

/// POST the ask request to a running `axon serve` instance and deserialize the
/// `AskResult` response. The server reuses its WarmSessionPool so ACP cold-start
/// is paid once at server boot, not per CLI invocation.
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
        serde_json::Value::Bool(cfg.ask_diagnostics),
    );
    payload.insert("graph".into(), serde_json::Value::Bool(cfg.ask_graph));
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

    let client = build_client(ASK_VIA_SERVER_TIMEOUT_SECS, None)
        .map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;
    let mut req = client.post(&endpoint).json(&payload);
    if let Ok(token) = std::env::var("AXON_MCP_HTTP_TOKEN")
        && !token.trim().is_empty()
    {
        // Cleartext-bearer guard: refuse to send the token over `http://` to a
        // non-loopback host unless the user explicitly opts in. The check lives
        // here (not in the type's constructor) because the token is read from
        // env at request time, not at config-build time.
        check_cleartext_token_allowed(server_url).map_err(|e| -> Box<dyn Error> { e.into() })?;
        req = req.bearer_auth(token.trim());
    }

    let resp = req
        .send()
        .await
        .map_err(|e| -> Box<dyn Error> { format!("connect to {endpoint}: {e}").into() })?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp
            .text()
            .await
            .unwrap_or_else(|e| format!("<body read failed: {e}>"));
        return Err(format!("server returned {status}: {body}").into());
    }
    let result: AskResult = resp.json().await.map_err(|e| -> Box<dyn Error> {
        format!("decode AskResult from {endpoint}: {e}").into()
    })?;
    Ok(result)
}

fn print_diagnostics(diag: &Option<crate::services::types::AskDiagnostics>) {
    let Some(diag) = diag else {
        return;
    };

    println!(
        "  {} candidates={} reranked={} chunks={} full_docs={} supplemental={} context_chars={} authority_ratio={:.2}",
        muted("Diagnostics:"),
        diag.candidate_pool,
        diag.reranked_pool,
        diag.chunks_selected,
        diag.full_docs_selected,
        diag.supplemental_selected,
        diag.context_chars,
        diag.authority_ratio,
    );

    if !diag.top_domains.is_empty() {
        println!(
            "  {} {}",
            muted("Top domains:"),
            diag.top_domains.join(", ")
        );
    }
}

#[cfg(test)]
mod ask_via_server_tests;
