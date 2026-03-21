use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::core::ui::{muted, primary};
use crate::crates::services::error::diagnostics_from_error;
use crate::crates::services::query as query_svc;
use std::error::Error;

fn resolve_ask_text(cfg: &Config) -> Option<String> {
    cfg.query
        .clone()
        .filter(|q| !q.trim().is_empty())
        .or_else(|| {
            if cfg.positional.is_empty() {
                None
            } else {
                Some(cfg.positional.join(" "))
            }
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub async fn run_ask(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let query = resolve_ask_text(cfg).ok_or("ask requires a question")?;
    log_info(&format!(
        "command=ask query_len={} collection={}",
        query.len(),
        cfg.collection
    ));

    let result = match query_svc::ask(cfg, &query, None).await {
        Ok(result) => result,
        Err(err) => {
            if cfg.ask_diagnostics
                && let Some(diag) = diagnostics_from_error(err.as_ref())
            {
                eprintln!("{} {}", muted("Diagnostics:"), diag);
            }
            return Err(err);
        }
    };
    let payload = &result.payload;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(payload)?);
        return Ok(());
    }

    // Human-readable output
    let answer = payload["answer"].as_str().unwrap_or("");

    println!("{}", primary("Conversation"));
    println!("  {} {}", primary("You:"), query);
    println!("  {} {}", primary("Assistant:"), answer);

    // Timing line
    if let Some(timing) = payload.get("timing_ms") {
        let retrieval = timing["retrieval"].as_u64().unwrap_or(0);
        let context = timing["context_build"].as_u64().unwrap_or(0);
        let llm = timing["llm"].as_u64().unwrap_or(0);
        let total = timing["total"].as_u64().unwrap_or(0);
        println!(
            "  {} retrieval={}ms | context={}ms | llm={}ms | total={}ms",
            muted("Timing:"),
            retrieval,
            context,
            llm,
            total
        );
    }

    // Diagnostics (only when enabled and present)
    if cfg.ask_diagnostics
        && let Some(diag) = payload.get("diagnostics")
        && !diag.is_null()
    {
        let candidates = diag["candidate_pool"].as_u64().unwrap_or(0);
        let reranked = diag["reranked_pool"].as_u64().unwrap_or(0);
        let chunks = diag["chunks_selected"].as_u64().unwrap_or(0);
        let full_docs = diag["full_docs_selected"].as_u64().unwrap_or(0);
        let supplemental = diag["supplemental_selected"].as_u64().unwrap_or(0);
        let context_chars = diag["context_chars"].as_u64().unwrap_or(0);
        let authority_ratio = diag["authority_ratio"].as_f64().unwrap_or(0.0);
        let dropped = diag["dropped_by_allowlist"].as_u64().unwrap_or(0);

        println!(
            "  {} candidates={} reranked={} chunks={} full_docs={} supplemental={} context_chars={} authority_ratio={:.2} dropped_by_allowlist={}",
            muted("Diagnostics:"),
            candidates,
            reranked,
            chunks,
            full_docs,
            supplemental,
            context_chars,
            authority_ratio,
            dropped
        );

        if let Some(domains) = diag["top_domains"].as_array() {
            let domain_list: Vec<&str> = domains.iter().filter_map(|d| d.as_str()).collect();
            if !domain_list.is_empty() {
                println!("  {} {}", muted("Top domains:"), domain_list.join(", "));
            }
        }
    }

    Ok(())
}
