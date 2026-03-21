use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::core::ui::{accent, muted, primary, status_text};
use crate::crates::services::error::diagnostics_from_error;
use crate::crates::services::query as query_svc;
use crate::crates::services::types::Pagination;
use std::error::Error;

fn resolve_query_text(cfg: &Config) -> Option<String> {
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

pub async fn run_query(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let query = resolve_query_text(cfg).ok_or("query requires text")?;
    log_info(&format!(
        "command=query query_len={} limit={}",
        query.len(),
        cfg.search_limit
    ));

    let opts = Pagination {
        limit: cfg.search_limit.max(1),
        offset: 0,
    };
    let results = match query_svc::query(cfg, &query, opts).await {
        Ok(result) => result.results,
        Err(err) => {
            if cfg.ask_diagnostics
                && let Some(diag) = diagnostics_from_error(err.as_ref())
            {
                eprintln!("{} {}", muted("Diagnostics:"), diag);
            }
            return Err(err);
        }
    };

    if results.is_empty() {
        if !cfg.json_output {
            println!("{}", primary(&format!("Query Results for \"{query}\"")));
            println!("  {}", muted("No results found."));
        }
        return Ok(());
    }

    if !cfg.json_output {
        println!("{}", primary(&format!("Query Results for \"{query}\"")));
        println!("{} {}\n", muted("Showing"), results.len());
    }

    for result in &results {
        let rank = result["rank"].as_u64().unwrap_or(0);
        let score = result["score"].as_f64().unwrap_or(0.0);
        let rerank_score = result["rerank_score"].as_f64().unwrap_or(0.0);
        let url = result["url"].as_str().unwrap_or("");
        let source = result["source"].as_str().unwrap_or("");
        let snippet = result["snippet"].as_str().unwrap_or("");

        if cfg.json_output {
            println!("{}", result);
        } else {
            println!(
                "  • {}. {} [{:.3}] {}",
                rank,
                status_text("completed"),
                rerank_score,
                accent(source)
            );
            println!("    {}", snippet);
            if cfg.ask_diagnostics {
                println!("    {} vector_score={:.3}", muted("diag"), score);
                println!("    {} {}", muted("url"), url);
            }
        }
    }

    Ok(())
}
