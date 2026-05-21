use anyhow::Result;

use crate::core::config::Config;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::ingest::progress::PhaseReporter;

mod client;
mod embed;
mod files;
mod types;

use client::{build_gitlab_client, fetch_project};
use embed::{embed_issues, embed_merge_requests, embed_metadata, embed_wiki};
use files::embed_files;

pub use types::{GitLabTarget, normalize_gitlab_target, parse_gitlab_target};

pub async fn ingest_gitlab(
    cfg: &Config,
    target: &str,
    include_source: bool,
    reporter: PhaseReporter,
) -> Result<usize> {
    log_info(&format!("command=ingest source=gitlab target={target}"));
    let target = parse_gitlab_target(target)?;
    let client = build_gitlab_client(cfg)?;
    let project = fetch_project(&client, &target).await?;
    reporter
        .report(serde_json::json!({
            "phase": "ingesting",
            "tasks_total": 5,
            "tasks_done": 0,
        }))
        .await;

    // Run each phase sequentially and report progress after each one completes.
    // (The previous array-literal approach evaluated all futures before iteration,
    // delaying all progress updates until the final phase finished.)
    let mut total = 0usize;
    let mut tasks_done = 0usize;
    const TASKS_TOTAL: usize = 5;

    macro_rules! run_phase {
        ($label:expr, $fut:expr) => {{
            reporter
                .report(
                    serde_json::json!({"phase": $label, "tasks_done": tasks_done, "tasks_total": TASKS_TOTAL}),
                )
                .await;
            match $fut.await {
                Ok(chunks) => {
                    total += chunks;
                    log_info(&format!(
                        "gitlab task_done task={} target={} chunks={chunks}",
                        $label,
                        target.namespace_path
                    ));
                }
                Err(err) => log_warn(&format!(
                    "gitlab task_failed task={} target={} err={err}",
                    $label,
                    target.namespace_path
                )),
            }
            tasks_done += 1;
        }};
    }

    run_phase!("metadata", embed_metadata(cfg, &target, &project));
    run_phase!(
        "files",
        embed_files(cfg, &target, &project, include_source, &reporter)
    );
    run_phase!("issues", embed_issues(cfg, &client, &target, &project));
    run_phase!(
        "merge_requests",
        embed_merge_requests(cfg, &client, &target, &project)
    );
    run_phase!("wiki", embed_wiki(cfg, &client, &target, &project));

    reporter
        .report(serde_json::json!({
            "tasks_done": tasks_done,
            "tasks_total": TASKS_TOTAL,
            "chunks_embedded": total,
            "phase": "completed",
        }))
        .await;
    log_done(&format!(
        "command=ingest source=gitlab target={} chunk_count={total}",
        target.namespace_path
    ));
    Ok(total)
}

#[cfg(test)]
#[path = "gitlab_tests.rs"]
mod tests;
