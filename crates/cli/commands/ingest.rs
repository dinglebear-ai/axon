use crate::crates::cli::commands::ingest_common;
use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::services::ingest as ingest_service;
use std::error::Error;

pub async fn run_ingest(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if ingest_common::maybe_handle_ingest_subcommand(cfg, "ingest").await? {
        return Ok(());
    }

    let target = cfg.positional.first().cloned().ok_or(
        "ingest requires a target. Examples:\n\
         \n\
         GitHub:   axon ingest owner/repo\n\
         Reddit:   axon ingest r/rust\n\
                   axon ingest https://reddit.com/r/rust/comments/...\n\
         YouTube:  axon ingest https://youtube.com/watch?v=...\n\
                   axon ingest @channelname\n\
         \n\
         Run 'axon ingest --help' for full usage.",
    )?;

    log_info(&format!("command=ingest target={target}"));
    let source = ingest_service::classify_target(&target, cfg.github_include_source)?;

    if !cfg.wait {
        let result = ingest_common::enqueue_ingest_job(cfg, source).await;
        if result.is_ok() {
            log_info(&format!(
                "job_enqueued command=ingest queue={}",
                cfg.ingest_queue
            ));
        }
        return result;
    }

    ingest_common::run_ingest_sync(cfg, source).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::CommandKind;
    use crate::crates::jobs::common::test_config;

    #[tokio::test]
    async fn run_ingest_requires_target() {
        let mut cfg = test_config("");
        cfg.command = CommandKind::Ingest;
        cfg.positional = vec![];
        let err = run_ingest(&cfg)
            .await
            .expect_err("expected missing target error");
        assert!(
            err.to_string().contains("ingest requires a target"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn run_ingest_unknown_target_gives_helpful_error() {
        let mut cfg = test_config("");
        cfg.command = CommandKind::Ingest;
        cfg.positional = vec!["not-a-target".to_string()];
        let err = run_ingest(&cfg)
            .await
            .expect_err("expected classification error");
        assert!(
            err.to_string().contains("cannot determine ingest source"),
            "unexpected error: {err}"
        );
    }
}
