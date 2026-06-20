use crate::cli::commands::resolve_input_text;
use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::core::ui::{accent, muted, primary};
use crate::services::context::ServiceContext;
use crate::services::query as query_svc;
use crate::services::types::{CodeSearchCaller, CodeSearchOptions};
use std::error::Error;

pub async fn run_code_search(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let query = resolve_input_text(cfg).ok_or("code-search requires text")?;
    if !cfg.json_output {
        log_info(&format!(
            "command=code-search query_len={} limit={}",
            query.len(),
            cfg.search_limit
        ));
    }

    let result = query_svc::code_search(
        service_context,
        &query,
        CodeSearchOptions {
            limit: cfg.search_limit.max(1),
            offset: 0,
            cwd: cfg.code_search_cwd.clone(),
            path_prefix: cfg.code_search_path_prefix.clone(),
            ensure_fresh: !cfg.code_search_no_freshness,
            caller: CodeSearchCaller::Cli,
        },
    )
    .await
    .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!(
        "{}",
        primary(&format!("Code Search Results for \"{query}\""))
    );
    if let Some(warning) = &result.freshness.warning {
        println!("{} {}", muted("freshness:"), warning);
    }
    if result.results.is_empty() {
        println!("  {}", muted("No results found."));
        return Ok(());
    }

    println!("{} {}\n", muted("Showing"), result.results.len());
    for hit in &result.results {
        let path = hit.file_path.as_deref().unwrap_or(hit.source.as_str());
        let line_suffix = match (hit.start_line, hit.end_line) {
            (Some(start), Some(end)) if end != start => format!(":{start}-{end}"),
            (Some(start), _) => format!(":{start}"),
            _ => String::new(),
        };
        let symbol = hit.symbol.as_deref().unwrap_or("");
        let symbol = if symbol.is_empty() {
            String::new()
        } else {
            format!(" {symbol}")
        };
        println!(
            "  {}. {}{}{} rerank={:.3}",
            hit.rank,
            accent(path),
            line_suffix,
            symbol,
            hit.rerank_score
        );
        println!("    {}", hit.snippet);
    }

    Ok(())
}
