use crate::cli::commands::resolve_input_text;
use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::core::ui::{muted, primary};
use crate::services::error::diagnostics_from_error;
use crate::services::query as query_svc;
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
