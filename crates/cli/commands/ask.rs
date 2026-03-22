use crate::crates::cli::commands::resolve_input_text;
use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::core::ui::{muted, primary};
use crate::crates::services::error::diagnostics_from_error;
use crate::crates::services::query as query_svc;
use std::error::Error;

pub async fn run_ask(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let query = resolve_input_text(cfg).ok_or("ask requires a question")?;
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

    println!("{}", primary("Conversation"));
    println!("  {} {}", primary("You:"), query);
    println!(
        "  {} {}",
        primary("Assistant:"),
        payload["answer"].as_str().unwrap_or("")
    );

    if let Some(timing) = payload.get("timing_ms") {
        println!(
            "  {} retrieval={}ms | context={}ms | llm={}ms | total={}ms",
            muted("Timing:"),
            timing["retrieval"].as_u64().unwrap_or(0),
            timing["context_build"].as_u64().unwrap_or(0),
            timing["llm"].as_u64().unwrap_or(0),
            timing["total"].as_u64().unwrap_or(0),
        );
    }

    if cfg.ask_diagnostics {
        print_diagnostics(payload);
    }

    Ok(())
}

fn print_diagnostics(payload: &serde_json::Value) {
    let Some(diag) = payload.get("diagnostics").filter(|d| !d.is_null()) else {
        return;
    };

    println!(
        "  {} candidates={} reranked={} chunks={} full_docs={} supplemental={} context_chars={} authority_ratio={:.2} dropped_by_allowlist={}",
        muted("Diagnostics:"),
        diag["candidate_pool"].as_u64().unwrap_or(0),
        diag["reranked_pool"].as_u64().unwrap_or(0),
        diag["chunks_selected"].as_u64().unwrap_or(0),
        diag["full_docs_selected"].as_u64().unwrap_or(0),
        diag["supplemental_selected"].as_u64().unwrap_or(0),
        diag["context_chars"].as_u64().unwrap_or(0),
        diag["authority_ratio"].as_f64().unwrap_or(0.0),
        diag["dropped_by_allowlist"].as_u64().unwrap_or(0),
    );

    if let Some(domains) = diag["top_domains"].as_array() {
        let domain_list: Vec<&str> = domains.iter().filter_map(|d| d.as_str()).collect();
        if !domain_list.is_empty() {
            println!("  {} {}", muted("Top domains:"), domain_list.join(", "));
        }
    }
}
