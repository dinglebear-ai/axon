use crate::cli::commands::CommandFuture;
use crate::cli::commands::ingest_common;
use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::core::ui::{accent, muted, primary, wait_spinner_for};
use crate::services::context::ServiceContext;
use crate::services::ingest::{self as ingest_service, IngestSource};
use crate::services::types::StartDisposition;
use std::error::Error;

pub(crate) fn render_ingest_enqueue_result(
    cfg: &Config,
    job_id: &str,
    disposition: StartDisposition,
    via_server: bool,
) -> Result<(), Box<dyn Error>> {
    let status = if disposition == StartDisposition::Completed {
        "completed"
    } else {
        "pending"
    };
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"job_id": job_id, "status": status}))?
        );
    } else {
        println!("  {} {}", primary("Ingest Job"), accent(job_id));
        if disposition == StartDisposition::Completed {
            let message = if via_server {
                "Server completed the ingest before returning."
            } else {
                "SQLite runtime completed the ingest in-process."
            };
            println!("  {}", muted(message));
        }
    }
    Ok(())
}

pub fn run_ingest<'a>(cfg: &'a Config, service_context: &'a ServiceContext) -> CommandFuture<'a> {
    Box::pin(async move {
        if ingest_common::maybe_handle_ingest_subcommand(cfg, service_context, "ingest").await? {
            return Ok(());
        }

        let target = cfg.positional.first().cloned().ok_or(
            "ingest requires a target. Examples:\n\
                 \n\
                 GitHub:   axon ingest owner/repo\n\
                 GitLab:   axon ingest https://gitlab.com/group/project\n\
                 Gitea:    axon ingest gitea:gitea.example.com/org/repo\n\
                 Git:      axon ingest git:https://example.com/org/repo.git\n\
                 Reddit:   axon ingest r/rust\n\
                           axon ingest https://reddit.com/r/rust/comments/...\n\
                 YouTube:  axon ingest https://youtube.com/watch?v=...\n\
                           axon ingest @channelname\n\
                 RSS/Atom: axon ingest https://blog.example.com/feed.xml\n\
                           axon ingest rss:example.com/feed\n\
                 \n\
                 Run 'axon ingest --help' for full usage.",
        )?;

        log_info(&format!("command=ingest target={target}"));
        let source = ingest_service::classify_target(&target, cfg.github_include_source)?;

        if !cfg.wait {
            let result = enqueue_ingest_job(cfg, source, service_context).await;
            if result.is_ok() {
                log_info("job_enqueued command=ingest");
            }
            return result;
        }

        let sp = wait_spinner_for(cfg, &format!("Ingesting {}…", target));
        let result = ingest_common::run_ingest_sync(cfg, source).await;
        if let Some(sp) = sp {
            sp.clear();
        }
        result
    })
}

async fn enqueue_ingest_job(
    cfg: &Config,
    source: IngestSource,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let outcome = ingest_service::ingest_start_with_context(cfg, source, service_context).await?;
    let job_id = outcome.result.job_id;
    let status = if outcome.disposition == StartDisposition::Completed {
        "completed"
    } else {
        "pending"
    };
    let _ = status;
    render_ingest_enqueue_result(cfg, &job_id, outcome.disposition, false)
}

#[cfg(test)]
#[path = "ingest_tests.rs"]
mod tests;
