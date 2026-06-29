use crate::commands::CommandFuture;
use crate::commands::code_search::run_code_search_watch;
use crate::commands::common::{
    handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_errors,
    handle_job_list_with_rows, handle_job_recover, handle_job_status, handle_worker_mode,
};
use crate::commands::fresh::create_schedule_from_command;
use crate::commands::job_progress::embed_progress_summary;
use crate::commands::status::metrics::{
    collection_from_config, display_embed_input, format_error, job_runtime_text,
};
use axon_core::config::{CodeSearchWatchConfig, Config};
use axon_core::logging::{log_done, log_info};
use axon_core::ui::wait_spinner_for;
use axon_core::ui::{accent, confirm_destructive, error, muted, primary, symbol_for_status};
use axon_jobs::backend::JobKind;
use axon_services::context::ServiceContext;
use axon_services::embed as embed_service;
use axon_services::jobs as job_service;
use axon_services::types::StartDisposition;
use std::error::Error;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

pub(crate) fn render_embed_list(
    cfg: &Config,
    all_jobs: Vec<axon_services::types::ServiceJob>,
    total: i64,
) -> Result<(), Box<dyn Error>> {
    let (limit, offset) = axon_services::transport::job_list_pagination(None, None);
    let result = axon_services::types::JobListResult::new(all_jobs, total, limit, offset);
    let empty_crawl_map = std::collections::HashMap::new();
    handle_job_list_with_rows(
        cfg,
        &result,
        "Embed",
        Some("No embed jobs found."),
        &[
            "",
            "ID",
            "Status",
            "Input",
            "Progress",
            "Collection",
            "Age",
            "Error",
        ],
        |job| {
            let target = display_embed_input(
                job.target.as_deref().unwrap_or(""),
                job.config_json.as_ref(),
                &empty_crawl_map,
            );
            let collection = collection_from_config(
                job.config_json.as_ref().unwrap_or(&serde_json::Value::Null),
            )
            .unwrap_or("");
            let age = job_runtime_text(
                &job.status,
                job.started_at.as_ref(),
                job.finished_at.as_ref(),
                &job.updated_at,
            );
            vec![
                symbol_for_status(&job.status),
                job.id.to_string(),
                axon_core::ui::status_text(&job.status),
                primary(&target).to_string(),
                embed_progress_summary(job, None)
                    .map(|summary| accent(&summary).to_string())
                    .unwrap_or_default(),
                accent(collection).to_string(),
                accent(&age).to_string(),
                format_error(job.error_text.as_deref())
                    .map(|err| error(&err).to_string())
                    .unwrap_or_default(),
            ]
        },
    )
}

pub(crate) fn render_embed_enqueue_result(
    cfg: &Config,
    input: &str,
    job_id: &str,
    disposition: StartDisposition,
    via_server: bool,
) {
    let status = if disposition == StartDisposition::Completed {
        "completed"
    } else {
        "pending"
    };
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({
                "job_id": job_id,
                "status": status,
                "target": input,
                "collection": cfg.collection,
                "source": "rust",
            })
        );
    } else {
        println!("  {} {}", primary("Embed Job"), accent(job_id));
        println!("  {}", muted(&format!("Input: {input}")));
        if disposition == StartDisposition::Completed {
            let message = if via_server {
                "Server completed the embed before returning."
            } else {
                "SQLite runtime completed the embed in-process."
            };
            println!("  {}", muted(message));
        }
        println!("Job ID: {job_id}");
    }
}

pub fn run_embed<'a>(cfg: &'a Config, service_context: &'a ServiceContext) -> CommandFuture<'a> {
    Box::pin(async move {
        if maybe_handle_embed_subcommand(cfg, service_context).await? {
            return Ok(());
        }
        if cfg.freshness.is_some() {
            return create_schedule_from_command(cfg, service_context).await;
        }

        log_info(&format!(
            "command=embed collection={} wait={}",
            cfg.collection, cfg.wait
        ));
        let embed_start = std::time::Instant::now();
        let input = resolve_embed_input(cfg);
        // A local path can only be embedded by a process that shares its
        // filesystem. A fire-and-forget CLI never services its own queue, so
        // an enqueued host path lands on whatever long-running worker exists —
        // usually the axon container, which cannot see the host home dir.
        // Local-path embeds therefore always run in-process here; only URL /
        // free-text inputs go through the shared queue when --wait is false.
        let input_is_local_path = Path::new(&input).exists();
        let effective_embed_watch =
            embed_should_watch(cfg.embed_watch, cfg.embed_no_watch, Path::new(&input));
        if effective_embed_watch {
            validate_embed_watch_input(&input, input_is_local_path)?;
        }
        if !cfg.wait && !input_is_local_path {
            let result = enqueue_embed_job(cfg, &input, service_context).await;
            if result.is_ok() {
                log_info("job_enqueued command=embed");
            }
            return result;
        }
        if !cfg.wait && input_is_local_path {
            let reason = if effective_embed_watch {
                "local_path_watch_runs_in_process"
            } else {
                "local_path_runs_in_process"
            };
            log_info(&format!("command=embed {reason}"));
        }
        if effective_embed_watch && !cfg.json_output {
            println!(
                "  {}",
                muted("Watching local code indexing refresh in the foreground.")
            );
        }
        if effective_embed_watch {
            let input_path = watch_root_for_embed_input(Path::new(&input));
            let initial_refresh = is_watchable_code_dir(&input_path);
            let mut watch_cfg = cfg.clone();
            watch_cfg.code_search_watch = Some(CodeSearchWatchConfig {
                roots: vec![input_path],
                debounce: Duration::from_millis(500),
                settle: Duration::from_secs(2),
                initial_refresh,
                dry_run: false,
                enable: false,
                json: cfg.json_output,
            });
            return run_code_search_watch(&watch_cfg, service_context).await;
        }

        let sp = wait_spinner_for(cfg, &format!("Embedding {}…", input));
        embed_service::embed_now(cfg, &input).await?;
        if let Some(sp) = sp {
            sp.finish("✓ Embedded");
        }
        log_done(&format!(
            "command=embed complete collection={} duration_ms={}",
            cfg.collection,
            embed_start.elapsed().as_millis()
        ));
        Ok(())
    })
}

async fn maybe_handle_embed_subcommand(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<bool, Box<dyn Error>> {
    let Some(subcmd) = cfg.positional.first().map(|s| s.as_str()) else {
        return Ok(false);
    };
    if cfg.positional.len() == 1 && Path::new(subcmd).exists() {
        // Allow embedding a local path literally named like a subcommand
        // (for example: "./status").
        return Ok(false);
    }

    match subcmd {
        "status" => handle_embed_status(cfg, service_context).await?,
        "cancel" => handle_embed_cancel(cfg, service_context).await?,
        "errors" => handle_embed_errors(cfg, service_context).await?,
        "list" => handle_embed_list(cfg, service_context).await?,
        "cleanup" => handle_embed_cleanup(cfg, service_context).await?,
        "clear" => handle_embed_clear(cfg, service_context).await?,
        "worker" => {
            handle_worker_mode(job_service::start_worker(service_context, JobKind::Embed).await?)?
        }
        "recover" => handle_embed_recover(cfg, service_context).await?,
        _ => return Ok(false),
    }

    Ok(true)
}

fn is_watchable_code_dir(path: &Path) -> bool {
    path.is_dir()
}

fn embed_should_watch(explicit_watch: bool, no_watch: bool, input: &Path) -> bool {
    !no_watch && (explicit_watch || input.exists())
}

fn watch_root_for_embed_input(input: &Path) -> PathBuf {
    let start = if input.is_file() {
        input.parent().unwrap_or(input)
    } else {
        input
    };
    nearest_git_root(start).unwrap_or_else(|| start.to_path_buf())
}

fn nearest_git_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(Path::to_path_buf)
}

fn parse_embed_job_id(cfg: &Config, action: &str) -> Result<Uuid, Box<dyn Error>> {
    let id = cfg
        .positional
        .get(1)
        .ok_or_else(|| format!("embed {action} requires <job-id>"))?;
    Ok(Uuid::parse_str(id)?)
}

fn validate_embed_watch_input(
    _input: &str,
    input_is_local_path: bool,
) -> Result<(), Box<dyn Error>> {
    if !input_is_local_path {
        return Err("embed watch mode requires a local file or directory".into());
    }
    Ok(())
}

async fn handle_embed_status(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let id = parse_embed_job_id(cfg, "status")?;
    let job = job_service::job_status(service_context, JobKind::Embed, id).await?;
    handle_job_status(cfg, job, id, "Embed")
}

async fn handle_embed_cancel(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let id = parse_embed_job_id(cfg, "cancel")?;
    let canceled = job_service::cancel_job(service_context, JobKind::Embed, id).await?;
    handle_job_cancel(cfg, id, canceled, "embed")
}

async fn handle_embed_errors(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let id = parse_embed_job_id(cfg, "errors")?;
    let job = job_service::job_status(service_context, JobKind::Embed, id).await?;
    handle_job_errors(cfg, job, id, "embed")
}

async fn handle_embed_list(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let (limit, offset) = axon_services::transport::job_list_pagination(None, None);
    let all_jobs = job_service::list_jobs(service_context, JobKind::Embed, limit, offset).await?;
    let total = all_jobs.len() as i64;
    render_embed_list(cfg, all_jobs, total)
}

async fn handle_embed_cleanup(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let removed = job_service::cleanup_jobs(service_context, JobKind::Embed).await?;
    handle_job_cleanup(cfg, removed, "embed")
}

async fn handle_embed_clear(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    if !confirm_destructive(cfg, "Clear all embed jobs and purge embed queue?")? {
        if cfg.json_output {
            println!("{}", serde_json::json!({ "removed": 0 }));
        } else {
            println!("{} aborted", symbol_for_status("canceled"));
        }
        return Ok(());
    }

    let removed = job_service::clear_jobs(service_context, JobKind::Embed).await?;
    handle_job_clear(cfg, removed, "embed")
}

async fn handle_embed_recover(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let reclaimed = job_service::recover_jobs(service_context, JobKind::Embed).await?;
    handle_job_recover(cfg, reclaimed, "embed")
}

fn resolve_embed_input(cfg: &Config) -> String {
    cfg.positional.first().cloned().unwrap_or_else(|| {
        cfg.output_dir
            .join("markdown")
            .to_string_lossy()
            .to_string()
    })
}

async fn enqueue_embed_job(
    cfg: &Config,
    input: &str,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let outcome =
        embed_service::embed_start_with_context(cfg, input, service_context, None, None).await?;
    let job_id = outcome.result.job_id;
    let status = if outcome.disposition == StartDisposition::Completed {
        "completed"
    } else {
        "pending"
    };
    let _ = status;
    render_embed_enqueue_result(cfg, input, &job_id, outcome.disposition, false);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        embed_should_watch, is_watchable_code_dir, validate_embed_watch_input,
        watch_root_for_embed_input,
    };

    #[test]
    fn embed_watch_rejects_non_local_inputs() {
        let err = validate_embed_watch_input("https://example.com/repo", false).unwrap_err();

        assert!(err.to_string().contains("local file or directory"));
    }

    #[test]
    fn embed_watch_accepts_files() -> Result<(), Box<dyn std::error::Error>> {
        let file = tempfile::NamedTempFile::new()?;
        let path = file.path().to_string_lossy();

        validate_embed_watch_input(&path, true)?;
        Ok(())
    }

    #[test]
    fn embed_watch_accepts_directories() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;
        let path = dir.path().to_string_lossy();

        validate_embed_watch_input(&path, true)?;
        Ok(())
    }

    #[test]
    fn embed_watch_initial_refreshes_git_checkouts_and_workspaces()
    -> Result<(), Box<dyn std::error::Error>> {
        let checkout = tempfile::TempDir::new()?;
        std::fs::write(checkout.path().join(".git"), "gitdir: ../real")?;
        let workspace = tempfile::TempDir::new()?;

        assert!(is_watchable_code_dir(checkout.path()));
        assert!(is_watchable_code_dir(workspace.path()));
        Ok(())
    }

    #[test]
    fn embed_watches_local_directories_by_default() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::TempDir::new()?;

        assert!(embed_should_watch(false, false, dir.path()));
        assert!(!embed_should_watch(false, true, dir.path()));
        Ok(())
    }

    #[test]
    fn embed_watches_local_files_by_default() -> Result<(), Box<dyn std::error::Error>> {
        let file = tempfile::NamedTempFile::new()?;

        assert!(embed_should_watch(false, false, file.path()));
        assert!(!embed_should_watch(false, true, file.path()));
        Ok(())
    }

    #[test]
    fn embed_watch_roots_file_inputs_at_nearest_git_checkout()
    -> Result<(), Box<dyn std::error::Error>> {
        let repo = tempfile::TempDir::new()?;
        std::fs::create_dir(repo.path().join(".git"))?;
        std::fs::create_dir_all(repo.path().join("src"))?;
        let file = repo.path().join("src/lib.rs");
        std::fs::write(&file, "fn main() {}\n")?;

        assert_eq!(watch_root_for_embed_input(&file), repo.path());
        Ok(())
    }
}
