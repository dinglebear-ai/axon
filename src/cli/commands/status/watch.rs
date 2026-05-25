//! `axon status --watch` — live MultiProgress view of running/pending jobs.
//!
//! Polls the same snapshot collector as the one-shot renderer every second
//! and reconciles a `HashMap<(kind, JobId), ProgressBar>` to mirror the active
//! set. Bars for jobs that leave the active set are emitted as a terminal
//! status line (`✓ completed url=…` / `✗ failed`) before being dropped, so
//! the user sees outcomes instead of bars silently vanishing.
//!
//! Two failure modes are explicit:
//! - Transient `load_status_jobs` errors log a warning and re-poll; the loop
//!   only exits via Ctrl-C or the idle-exit branch below.
//! - The loop self-terminates after `IDLE_EXIT_TICKS` consecutive idle polls
//!   (no active jobs seen at all yet AND no bars currently displayed) with an
//!   explicit final message — scripted callers see a clean Ok exit and a
//!   user-readable reason.

use crate::core::config::Config;
use crate::core::logging::log_warn;
use crate::core::ui::{accent, muted, primary, status_text, symbol_for_status};
use crate::services::context::ServiceContext;
use crate::services::system::load_status_jobs;
use crate::services::types::ServiceJob;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::time::Duration;

const TICK_MS: u64 = 100;
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const IDLE_EXIT_TICKS: u8 = 5;

type BarKey = (&'static str, String);

pub async fn run_status_watch(
    _cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let mp = MultiProgress::new();
    // Static templates — fail loudly if a developer breaks the literal during
    // a refactor, rather than silently degrading to default_spinner().
    let bar_style = ProgressStyle::with_template("{spinner:.cyan} {prefix:<8} {wide_msg}")
        .expect("static spinner template is malformed — fix the literal in watch.rs");
    let header_style = ProgressStyle::with_template("{msg}")
        .expect("static header template is malformed — fix the literal in watch.rs");

    let header_bar = mp.add(ProgressBar::new_spinner());
    header_bar.set_style(header_style);
    header_bar.set_message(format!(
        "{} (Ctrl-C to exit)",
        primary("axon status — live view"),
    ));

    let mut bars: HashMap<BarKey, ProgressBar> = HashMap::new();
    let mut idle_ticks: u8 = 0;

    loop {
        // Snapshot the state. Transient backend blips should NOT kill the
        // session — log and continue. Real bugs (e.g. malformed payload) still
        // surface eventually as a sustained log warning the user can react to.
        let jobs = match load_status_jobs(service_context).await {
            Ok((jobs, _totals, _errors)) => jobs,
            Err(err) => {
                log_warn(&format!("status watch: load_status_jobs failed: {err}"));
                tokio::time::sleep(POLL_INTERVAL).await;
                continue;
            }
        };
        let mut seen: HashSet<BarKey> = HashSet::new();

        let mut active_count = 0usize;
        for (kind, job) in iter_jobs(&jobs) {
            if !is_active(&job.status) {
                continue;
            }
            active_count += 1;
            let key: BarKey = (kind, job.id.to_string());
            seen.insert(key.clone());
            let bar = bars.entry(key).or_insert_with(|| {
                let pb = mp.add(ProgressBar::new_spinner());
                pb.set_style(bar_style.clone());
                pb.enable_steady_tick(Duration::from_millis(TICK_MS));
                pb
            });
            bar.set_prefix(kind.to_string());
            bar.set_message(format_subject(job));
        }

        // Look up terminal-state info for bars that just left the active set
        // so we can print an outcome line before clearing the bar.
        bars.retain(|key, bar| {
            if seen.contains(key) {
                return true;
            }
            let outcome = lookup_outcome(&jobs, key);
            match outcome {
                Some((status, subject)) => {
                    bar.finish_with_message(format!(
                        "{} {} {}",
                        symbol_for_status(&status),
                        status_text(&status),
                        muted(&subject),
                    ));
                }
                None => bar.finish_and_clear(),
            }
            false
        });

        if active_count == 0 && bars.is_empty() {
            idle_ticks = idle_ticks.saturating_add(1);
            header_bar.set_message(format!(
                "{} (no active jobs — Ctrl-C to exit)",
                accent("axon status"),
            ));
            if idle_ticks >= IDLE_EXIT_TICKS {
                header_bar.finish_with_message(format!(
                    "{} no active jobs for {}s; exiting.",
                    muted("axon status:"),
                    IDLE_EXIT_TICKS,
                ));
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

fn lookup_outcome(
    jobs: &crate::services::system::StatusJobs,
    key: &BarKey,
) -> Option<(String, String)> {
    let (kind, id) = key;
    let pool: &[ServiceJob] = match *kind {
        "crawl" => &jobs.crawl,
        "extract" => &jobs.extract,
        "embed" => &jobs.embed,
        "ingest" => &jobs.ingest,
        _ => return None,
    };
    pool.iter()
        .find(|j| j.id.to_string() == *id)
        .map(|j| (j.status.clone(), format_subject(j)))
}

fn format_subject(job: &ServiceJob) -> String {
    match (
        job.url.as_deref(),
        job.source_type.as_deref(),
        job.target.as_deref(),
    ) {
        (Some(url), _, _) => url.to_string(),
        (None, Some(st), Some(tgt)) => format!("{st}: {tgt}"),
        (None, _, Some(tgt)) => tgt.to_string(),
        _ => job.id.to_string(),
    }
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
