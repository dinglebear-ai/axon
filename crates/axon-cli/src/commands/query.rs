use crate::commands::resolve_input_text;
use axon_core::config::Config;
use axon_core::error::diagnostics_from_error;
use axon_core::logging::log_info;
use axon_core::ui::{accent, muted, primary, status_text};
use axon_services::context::ServiceContext;
use axon_services::query as query_svc;
use axon_services::types::Pagination;
use std::error::Error;

pub async fn run_query(cfg: &Config, ctx: &ServiceContext) -> Result<(), Box<dyn Error>> {
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
    let results = query_svc::query(ctx, cfg, &query, opts)
        .await
        .inspect_err(|err| {
            if cfg.ask_diagnostics
                && let Some(diag) = diagnostics_from_error(err.as_ref())
            {
                eprintln!("{} {}", muted("Diagnostics:"), diag);
            }
        })?
        .results;

    if cfg.json_output {
        for result in &results {
            println!("{}", serde_json::to_string(result)?);
        }
        return Ok(());
    }

    println!("{}", primary(&format!("Query Results for \"{query}\"")));

    if results.is_empty() {
        println!("  {}", muted("No results found. Try:"));
        println!("    {}", muted("axon sources       # list indexed URLs"));
        println!(
            "    {}",
            muted("axon stats         # check collection size")
        );
        println!("    {}", muted("axon embed <url>   # add content first"));
        return Ok(());
    }

    println!("{} {}\n", muted("Showing"), results.len());
    for result in &results {
        println!(
            "  \u{2022} {}. {} rerank={:.3} {}",
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

    Ok(())
}
