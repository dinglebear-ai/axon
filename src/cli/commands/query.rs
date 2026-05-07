use crate::cli::commands::resolve_input_text;
use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::core::ui::{accent, muted, primary, status_text};
use crate::services::error::diagnostics_from_error;
use crate::services::query as query_svc;
use crate::services::types::Pagination;
use std::error::Error;

pub async fn run_query(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let query = resolve_input_text(cfg).ok_or("query requires text")?;
    // TODO: cfg.quiet — suppress progress log when quiet mode lands
    if !cfg.json_output {
        log_info(&format!(
            "command=query query_len={} limit={}",
            query.len(),
            cfg.search_limit
        ));
    }

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
            println!("  {}", muted("No results found. Try:"));
            println!("    {}", muted("axon sources       # list indexed URLs"));
            println!(
                "    {}",
                muted("axon stats         # check collection size")
            );
            println!("    {}", muted("axon embed <url>   # add content first"));
        }
        return Ok(());
    }

    if !cfg.json_output {
        println!("{}", primary(&format!("Query Results for \"{query}\"")));
        println!("{} {}\n", muted("Showing"), results.len());
    }

    for result in &results {
        if cfg.json_output {
            println!("{}", serde_json::to_string(result)?);
        } else {
            println!(
                "  \u{2022} {}. {} [{:.3}] {}",
                result.rank,
                status_text("completed"),
                result.rerank_score,
                accent(&result.source)
            );
            println!("    {}", result.snippet);
            if cfg.ask_diagnostics {
                println!("    {} vector_score={:.3}", muted("diag"), result.score);
                println!("    {} {}", muted("url"), result.url);
            }
        }
    }

    Ok(())
}
