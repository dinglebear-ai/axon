use crate::crates::cli::commands::resolve_input_text;
use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::log_info;
use crate::crates::core::ui::{muted, primary};
use crate::crates::services::error::diagnostics_from_error;
use crate::crates::services::query as query_svc;
use crate::crates::services::types::AskResult;
use std::error::Error;

pub async fn run_ask(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let query = resolve_input_text(cfg).ok_or("ask requires a question")?;
    log_info(&format!(
        "command=ask query_len={} collection={} server_url={}",
        query.len(),
        cfg.collection,
        cfg.server_url.as_deref().unwrap_or("(in-process)")
    ));

    let result = if let Some(server_url) = cfg.server_url.as_deref() {
        match ask_via_server(cfg, server_url, &query).await {
            Ok(result) => result,
            Err(err) => {
                eprintln!(
                    "{} ask failed via server-url '{server_url}': {err}\n  hint: ensure `axon serve` is running there, or unset --server-url / AXON_ASK_SERVER_URL to fall back to in-process ask.",
                    muted("Error:")
                );
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

/// POST the ask request to a running `axon serve` instance and deserialize the
/// `AskResult` response. The server reuses its WarmSessionPool so ACP cold-start
/// is paid once at server boot, not per CLI invocation.
async fn ask_via_server(
    cfg: &Config,
    server_url: &str,
    query: &str,
) -> Result<AskResult, Box<dyn Error>> {
    let endpoint = format!("{}/v1/ask", server_url.trim_end_matches('/'));
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

    let client = http_client().map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;
    let mut req = client.post(&endpoint).json(&payload);
    if let Ok(token) = std::env::var("AXON_MCP_HTTP_TOKEN")
        && !token.trim().is_empty()
    {
        req = req.bearer_auth(token.trim());
    }

    let resp = req
        .send()
        .await
        .map_err(|e| -> Box<dyn Error> { format!("connect to {endpoint}: {e}").into() })?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("server returned {status}: {body}").into());
    }
    let result: AskResult = resp.json().await.map_err(|e| -> Box<dyn Error> {
        format!("decode AskResult from {endpoint}: {e}").into()
    })?;
    Ok(result)
}

fn print_diagnostics(diag: &Option<crate::crates::services::types::AskDiagnostics>) {
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
