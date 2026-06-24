//! Drive a `watch` tick: detect per-URL changes, summarize + record artifacts,
//! cluster changed URLs, and dispatch one crawl per cluster (in-flight-guarded).

use crate::watch::WatchDef;
use crate::watch::change_detect::{UrlOutcome, detect_url_change};
use crate::watch::cluster::group_by_common_prefix;
use crate::watch::dispatch::{crawl_job_active, enqueue_change_crawl};
use crate::watch::filter::compile_patterns;
use crate::watch::report::{summarize_diff, write_change_artifact};
use crate::watch::url_state::{get_url_state, set_crawl_job_id};
use axon_core::config::Config;
use sqlx::SqlitePool;
use uuid::Uuid;

/// Cluster changed URLs and dispatch one crawl per cluster, skipping clusters
/// whose prior crawl is still in flight. Records the new crawl id on members.
/// Returns `(clusters_out, dispatched, cluster_errors)`.
#[allow(clippy::type_complexity)]
pub(crate) async fn dispatch_clusters(
    pool: &SqlitePool,
    cfg: &Config,
    watch_id: Uuid,
    changed: &[String],
    max_depth: usize,
) -> (Vec<serde_json::Value>, Vec<String>, Vec<serde_json::Value>) {
    let mut clusters_out: Vec<serde_json::Value> = Vec::new();
    let mut dispatched: Vec<String> = Vec::new();
    let mut errors: Vec<serde_json::Value> = Vec::new();
    for cluster in group_by_common_prefix(changed) {
        let mut in_flight = false;
        for member in &cluster.members {
            if let Ok(Some(state)) = get_url_state(pool, watch_id, member).await
                && let Some(job_id) = state.last_crawl_job_id
                && crawl_job_active(pool, job_id).await
            {
                in_flight = true;
                break;
            }
        }
        if in_flight {
            clusters_out.push(serde_json::json!({
                "seed": cluster.seed, "members": cluster.members, "skipped": "crawl in flight"
            }));
            continue;
        }
        // Map the (non-Send) boxed enqueue error to a String at the await
        // boundary so the non-Send type never spans the upsert awaits below —
        // this future must stay Send for the scheduler's tokio::spawn.
        let enqueued = enqueue_change_crawl(pool, cfg, &cluster.seed, max_depth)
            .await
            .map_err(|e| e.to_string());
        match enqueued {
            Ok(job_id) => {
                // Targeted update of just last_crawl_job_id — never a full-row
                // upsert, which could clobber the snapshot detect_url_change
                // just wrote earlier this tick.
                for member in &cluster.members {
                    if let Err(e) = set_crawl_job_id(pool, watch_id, member, job_id).await {
                        tracing::warn!(%watch_id, url = member, %job_id, error = %e, "watch: set_crawl_job_id failed");
                    }
                }
                dispatched.push(job_id.to_string());
                clusters_out.push(serde_json::json!({
                    "seed": cluster.seed, "members": cluster.members, "crawl_job_id": job_id.to_string()
                }));
            }
            Err(msg) => errors.push(serde_json::json!({ "seed": cluster.seed, "error": msg })),
        }
    }
    (clusters_out, dispatched, errors)
}

pub(crate) async fn run_url_watch(
    cfg: &Config,
    pool: &SqlitePool,
    run_id: Uuid,
    watch: &WatchDef,
) -> Result<serde_json::Value, String> {
    let p = &watch.task_payload;
    let urls: Vec<String> = p
        .get("urls")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if urls.is_empty() {
        return Err("watch task requires task_payload.urls".to_string());
    }
    let max_depth = p
        .get("max_depth")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(2);
    let threshold = p
        .get("change_threshold_words")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let do_summary = p.get("summarize").and_then(|v| v.as_bool()).unwrap_or(true);
    let ignore_src: Vec<String> = p
        .get("ignore_patterns")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let ignore = compile_patterns(&ignore_src).map_err(|e| format!("watch: {e}"))?;

    let mut changed: Vec<String> = Vec::new();
    let mut unchanged = 0usize;
    let mut errors: Vec<serde_json::Value> = Vec::new();
    let mut summaries: Vec<serde_json::Value> = Vec::new();

    for url in &urls {
        match detect_url_change(cfg, pool, watch.id, url, &ignore, threshold).await {
            UrlOutcome::Failed { error } => {
                errors.push(serde_json::json!({ "url": url, "error": error }));
            }
            UrlOutcome::Unchanged => {
                unchanged += 1;
            }
            UrlOutcome::Changed { diff } => {
                changed.push(url.clone());
                let summary = if do_summary {
                    summarize_diff(cfg, url, &diff).await
                } else {
                    None
                };
                if let Some(s) = &summary {
                    summaries.push(serde_json::json!({ "url": url, "summary": s }));
                }
                if let Err(e) = write_change_artifact(pool, run_id, url, &diff, summary).await {
                    tracing::warn!(watch_id = %watch.id, url, error = %e, "watch: write_change_artifact failed");
                }
            }
        }
    }

    let (clusters_out, dispatched, cluster_errors) =
        dispatch_clusters(pool, cfg, watch.id, &changed, max_depth).await;
    errors.extend(cluster_errors);

    Ok(serde_json::json!({
        "mode": "url-change-watch",
        "checked": urls.len(),
        "changed": changed.len(),
        "unchanged": unchanged,
        "clusters": clusters_out,
        "dispatched": dispatched,
        "summaries": summaries,
        "errors": errors,
    }))
}

#[cfg(test)]
#[path = "orchestrate_tests.rs"]
mod tests;
