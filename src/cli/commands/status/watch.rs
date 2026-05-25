//! `axon status --watch` — live MultiProgress view of running/pending jobs.
//!
//! Polls the same snapshot collector as the one-shot renderer every second
//! and reconciles a `HashMap<JobId, ProgressBar>` to mirror the active set.
//! Terminal-state (completed/failed/canceled) jobs cause their bar to drop.

use crate::core::config::Config;
use crate::core::ui::{accent, primary};
use crate::services::context::ServiceContext;
use crate::services::system::load_status_jobs;
use crate::services::types::ServiceJob;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::time::Duration;

const TICK_MS: u64 = 100;
const POLL_INTERVAL: Duration = Duration::from_secs(1);

pub async fn run_status_watch(
    _cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let mp = MultiProgress::new();
    let style = ProgressStyle::with_template("{spinner:.cyan} {prefix:<8} {wide_msg}")
        .unwrap_or_else(|_| ProgressStyle::default_spinner());

    let header_bar = mp.add(ProgressBar::new_spinner());
    header_bar.set_style(
        ProgressStyle::with_template("{msg}").unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    header_bar.set_message(format!(
        "{} (Ctrl-C to exit)",
        primary("axon status — live view"),
    ));

    let mut bars: HashMap<String, ProgressBar> = HashMap::new();
    let mut idle_ticks: u8 = 0;

    loop {
        let (jobs, _totals, _errors) = load_status_jobs(service_context).await?;
        let mut seen: HashSet<String> = HashSet::new();

        for (kind, job) in iter_jobs(&jobs) {
            if !is_active(&job.status) {
                continue;
            }
            let id = job.id.to_string();
            seen.insert(id.clone());
            let bar = bars.entry(id.clone()).or_insert_with(|| {
                let pb = mp.add(ProgressBar::new_spinner());
                pb.set_style(style.clone());
                pb.enable_steady_tick(Duration::from_millis(TICK_MS));
                pb
            });
            bar.set_prefix(kind.to_string());
            bar.set_message(format_subject(job));
        }

        bars.retain(|id, bar| {
            if seen.contains(id) {
                true
            } else {
                bar.finish_and_clear();
                false
            }
        });

        if bars.is_empty() {
            idle_ticks = idle_ticks.saturating_add(1);
            header_bar.set_message(format!(
                "{} (no active jobs — Ctrl-C to exit)",
                accent("axon status"),
            ));
            // Exit after ~5 idle seconds so scripted use doesn't hang.
            if idle_ticks >= 5 {
                header_bar.finish_and_clear();
                return Ok(());
            }
        } else {
            idle_ticks = 0;
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn is_active(status: &str) -> bool {
    matches!(
        status,
        "running" | "pending" | "processing" | "scraping" | "claimed"
    )
}

fn iter_jobs(
    jobs: &crate::services::system::StatusJobs,
) -> impl Iterator<Item = (&'static str, &ServiceJob)> {
    let crawl = jobs.crawl.iter().map(|j| ("crawl", j));
    let extract = jobs.extract.iter().map(|j| ("extract", j));
    let embed = jobs.embed.iter().map(|j| ("embed", j));
    let ingest = jobs.ingest.iter().map(|j| ("ingest", j));
    crawl.chain(extract).chain(embed).chain(ingest)
}

fn format_subject(job: &ServiceJob) -> String {
    if let Some(url) = job.url.as_deref() {
        return url.to_string();
    }
    if let (Some(st), Some(tgt)) = (job.source_type.as_deref(), job.target.as_deref()) {
        return format!("{st}: {tgt}");
    }
    if let Some(t) = job.target.as_deref() {
        return t.to_string();
    }
    job.id.to_string()
}
