use crate::commands::resolve_input_text;
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{
    accent, error, metric, muted, primary, status_text, success, symbol_for_status, warning,
};
use axon_services::code_search_watch::{
    CodeSearchWatchDryRunPlan, CodeSearchWatchEvent, CodeSearchWatchEventSink, ReindexProgress,
    ReindexProgressSink, run_code_search_watch as run_code_search_watch_service,
};
use axon_services::context::ServiceContext;
use axon_services::query as query_svc;
use axon_services::types::{CodeSearchCaller, CodeSearchOptions};
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

    let progress = CliCodeSearchProgressSink;
    let progress = (!cfg.json_output).then_some(&progress as &dyn ReindexProgressSink);
    let result = query_svc::code_search_with_progress(
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
        progress,
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

pub async fn run_code_search_watch(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let options = cfg
        .code_search_watch
        .clone()
        .ok_or("code-search-watch requires watcher options")?;
    let sink = CliCodeSearchWatchEventSink { json: options.json };
    run_code_search_watch_service(service_context, options, &sink)
        .await
        .map_err(|err| -> Box<dyn Error> { err.to_string().into() })
}

struct CliCodeSearchWatchEventSink {
    json: bool,
}

struct CliCodeSearchProgressSink;

impl ReindexProgressSink for CliCodeSearchProgressSink {
    fn emit(&self, progress: ReindexProgress) {
        render_code_search_refresh_progress(progress);
    }
}

impl CodeSearchWatchEventSink for CliCodeSearchWatchEventSink {
    fn emit(&self, event: CodeSearchWatchEvent) {
        if self.json {
            emit_code_search_watch_json(event);
        } else {
            render_code_search_watch_event(event);
        }
    }
}

fn emit_code_search_watch_json(event: CodeSearchWatchEvent) {
    match serde_json::to_string(&event) {
        Ok(line) => println!("{line}"),
        Err(error) => println!(
            "{{\"event\":\"serialization_failed\",\"error\":{}}}",
            serde_json::json!(error.to_string())
        ),
    }
}

fn render_code_search_watch_event(event: CodeSearchWatchEvent) {
    match event {
        CodeSearchWatchEvent::Started {
            watch_dirs,
            roots,
            initial_refresh,
        } => {
            println!("{}", primary("Code Search Watcher"));
            println!(
                "  {} {} {}",
                symbol_for_status("running"),
                metric(roots.len(), if roots.len() == 1 { "repo" } else { "repos" }),
                muted(&format!("watching {} dir(s)", watch_dirs.len())),
            );
            if initial_refresh {
                println!("  {} {}", muted("Mode:"), accent("initial refresh"));
            }
        }
        CodeSearchWatchEvent::Pending { root, paths } => {
            println!(
                "  {} {} {}",
                symbol_for_status("pending"),
                accent("Queued changes"),
                muted(&format!("{} · {paths} path(s)", root.display())),
            );
        }
        CodeSearchWatchEvent::RefreshStarted { root, reason } => {
            println!();
            println!("{}", primary("Refresh"));
            println!(
                "  {} {} {}",
                symbol_for_status("running"),
                accent(root.to_string_lossy().as_ref()),
                muted(reason),
            );
        }
        CodeSearchWatchEvent::RefreshProgress { progress } => {
            render_code_search_refresh_progress(progress);
        }
        CodeSearchWatchEvent::RefreshFinished {
            root,
            status,
            warning: freshness_warning,
            indexed_files,
            removed_files,
            generation,
        } => {
            let outcome = if freshness_warning.is_some() {
                "warning"
            } else {
                "completed"
            };
            let generation = generation
                .map(|generation| generation.to_string())
                .unwrap_or_else(|| "none".to_string());
            println!(
                "  {} {} {}",
                symbol_for_status(outcome),
                status_text(outcome),
                muted(&format!(
                    "{} · status={status} · indexed={indexed_files} · removed={removed_files} · generation={generation}",
                    root.display()
                )),
            );
            if let Some(message) = freshness_warning {
                println!("    {} {}", warning("warning:"), muted(&message));
            }
        }
        CodeSearchWatchEvent::RefreshFailed { root, error: err } => {
            println!(
                "  {} {} {}",
                symbol_for_status("failed"),
                error("Refresh failed"),
                muted(&format!("{} · {err}", root.display())),
            );
        }
        CodeSearchWatchEvent::DryRun { plan } => render_code_search_watch_dry_run(plan),
        CodeSearchWatchEvent::Stopped => {
            println!(
                "  {} {}",
                symbol_for_status("completed"),
                muted("Watcher stopped")
            );
        }
    }
}

fn render_code_search_refresh_progress(progress: ReindexProgress) {
    match progress {
        ReindexProgress::Started {
            generation,
            total_files,
            added_files,
            modified_files,
            removed_files,
            total_batches,
        } => {
            println!("  {}", primary("Plan"));
            println!(
                "    {} {}",
                muted("Generation:"),
                accent(&generation.to_string())
            );
            println!(
                "    {} {}",
                muted("Files:"),
                accent(&format!(
                    "{total_files} total · {added_files} added · {modified_files} modified · {removed_files} removed"
                )),
            );
            println!(
                "    {} {}",
                muted("Batches:"),
                accent(&total_batches.to_string())
            );
        }
        ReindexProgress::BatchFinished {
            generation: _,
            batch_number,
            total_batches,
            processed_files,
            total_files,
            batch_files,
            embedded_docs,
        } => {
            let pct = progress_percent(processed_files, total_files);
            let previous_files = processed_files.saturating_sub(batch_files);
            let previous_pct = progress_percent(previous_files, total_files);
            let crossed_five_percent = pct != previous_pct && pct.is_multiple_of(5);
            if batch_number != 1 && batch_number != total_batches && !crossed_five_percent {
                return;
            }
            println!(
                "  {} {} {}",
                symbol_for_status("running"),
                accent(&format!("{processed_files}/{total_files} files")),
                muted(&format!(
                    "{pct}% · batch {batch_number}/{total_batches} · {embedded_docs} docs"
                )),
            );
        }
        ReindexProgress::CleanupStarted {
            generation: _,
            cleanup_paths,
        } => {
            println!(
                "  {} {}",
                muted("Cleanup:"),
                accent(&format!("{cleanup_paths} path(s)")),
            );
        }
        ReindexProgress::CommitStarted { generation } => {
            println!(
                "  {} {}",
                muted("Commit:"),
                accent(&format!("generation {generation}"))
            );
        }
        ReindexProgress::Finished { generation } => {
            println!(
                "  {} {}",
                success("✓ Indexed"),
                muted(&format!("generation {generation}"))
            );
        }
    }
}

fn progress_percent(done: usize, total: usize) -> usize {
    done.saturating_mul(100).checked_div(total).unwrap_or(100)
}

fn render_code_search_watch_dry_run(plan: CodeSearchWatchDryRunPlan) {
    println!("{}", primary("Code Search Dry Run"));
    println!(
        "  {} {}",
        metric(
            plan.roots.len(),
            if plan.roots.len() == 1 {
                "repo"
            } else {
                "repos"
            }
        ),
        muted(&format!("{} file(s)", plan.total_files)),
    );
    for root in plan.roots {
        println!("  {}", accent(&root.root.display().to_string()));
        for file in root.files {
            println!("    {} {}", muted("•"), muted(&file));
        }
    }
}
