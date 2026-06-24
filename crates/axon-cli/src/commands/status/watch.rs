//! `axon status --watch` — live MultiProgress view of running/pending jobs.
//!
//! Polls the same snapshot collector as the one-shot renderer every second
//! and reconciles a `HashMap<(kind, JobId), ProgressBar>` to mirror the active
//! set. Bars for jobs that leave the active snapshot are resolved through a
//! direct status lookup before removal, so terminal outcomes remain visible
//! even when the first status page is saturated.
//!
//! Two failure modes are explicit:
//! - Transient `load_status_jobs` errors log a warning and re-poll; the loop
//!   only exits via Ctrl-C or the idle-exit branch below.
//! - The loop self-terminates after `IDLE_EXIT_TICKS` consecutive idle polls
//!   (no active jobs seen at all yet AND no bars currently displayed) with an
//!   explicit final message — scripted callers see a clean Ok exit and a
//!   user-readable reason.

use axon_core::config::Config;
use axon_core::logging::log_warn;
use axon_core::ui::{accent, muted, primary, status_text, symbol_for_status};
use axon_jobs::backend::JobKind;
use axon_services::context::ServiceContext;
use axon_services::jobs as job_service;
use axon_services::system::load_status_jobs;
use axon_services::types::ServiceJob;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::time::Duration;
use uuid::Uuid;

const TICK_MS: u64 = 100;
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const IDLE_EXIT_TICKS: u8 = 3;

type BarKey = (JobKind, Uuid);

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
            if !is_active(job) {
                continue;
            }
            active_count += 1;
            let key: BarKey = (kind, job.id);
            seen.insert(key);
            let bar = bars.entry(key).or_insert_with(|| {
                let pb = mp.add(ProgressBar::new_spinner());
                pb.set_style(bar_style.clone());
                pb.enable_steady_tick(Duration::from_millis(TICK_MS));
                pb
            });
            bar.set_prefix(kind_label(kind).to_string());
            bar.set_message(format_subject(job));
        }

        let stale_keys: Vec<BarKey> = bars
            .keys()
            .filter(|key| !seen.contains(*key))
            .copied()
            .collect();
        for key in stale_keys {
            match resolve_stale_job(service_context, &jobs, key).await {
                StaleJob::StillActive(job) => {
                    if let Some(bar) = bars.get_mut(&key) {
                        bar.set_prefix(kind_label(key.0).to_string());
                        bar.set_message(format_subject(&job));
                    }
                }
                StaleJob::Terminal { status, subject } => {
                    if let Some(bar) = bars.remove(&key) {
                        bar.finish_with_message(format!(
                            "{} {} {}",
                            symbol_for_status(&status),
                            status_text(&status),
                            muted(&subject),
                        ));
                    }
                }
                StaleJob::Unknown(reason) => {
                    if let Some(bar) = bars.remove(&key) {
                        bar.finish_with_message(format!(
                            "{} {} {}",
                            symbol_for_status("failed"),
                            status_text("unknown"),
                            muted(&reason),
                        ));
                    }
                }
            }
        }

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

fn is_active(job: &ServiceJob) -> bool {
    job.status_enum().is_active()
}

fn iter_jobs(
    jobs: &axon_services::system::StatusJobs,
) -> impl Iterator<Item = (JobKind, &ServiceJob)> {
    let crawl = jobs.crawl.iter().map(|j| (JobKind::Crawl, j));
    let extract = jobs.extract.iter().map(|j| (JobKind::Extract, j));
    let embed = jobs.embed.iter().map(|j| (JobKind::Embed, j));
    let ingest = jobs.ingest.iter().map(|j| (JobKind::Ingest, j));
    crawl.chain(extract).chain(embed).chain(ingest)
}

fn lookup_outcome(
    jobs: &axon_services::system::StatusJobs,
    key: &BarKey,
) -> Option<(String, String)> {
    let (kind, id) = *key;
    let pool: &[ServiceJob] = match kind {
        JobKind::Crawl => &jobs.crawl,
        JobKind::Extract => &jobs.extract,
        JobKind::Embed => &jobs.embed,
        JobKind::Ingest => &jobs.ingest,
    };
    pool.iter()
        .find(|j| j.id == id)
        .map(|j| (j.status.clone(), format_subject(j)))
}

enum StaleJob {
    StillActive(Box<ServiceJob>),
    Terminal { status: String, subject: String },
    Unknown(String),
}

async fn resolve_stale_job(
    service_context: &ServiceContext,
    jobs: &axon_services::system::StatusJobs,
    key: BarKey,
) -> StaleJob {
    if let Some((status, subject)) = lookup_outcome(jobs, &key) {
        return StaleJob::Terminal { status, subject };
    }

    match job_service::job_status(service_context, key.0, key.1).await {
        Ok(Some(job)) if is_active(&job) => StaleJob::StillActive(Box::new(job)),
        Ok(Some(job)) => StaleJob::Terminal {
            status: job.status.clone(),
            subject: format_subject(&job),
        },
        Ok(None) => StaleJob::Unknown(format!(
            "{} {} left the active snapshot; current status is unavailable",
            kind_label(key.0),
            key.1
        )),
        Err(err) => StaleJob::Unknown(format!(
            "{} {} left the active snapshot; status lookup failed: {err}",
            kind_label(key.0),
            key.1
        )),
    }
}

fn kind_label(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Crawl => "crawl",
        JobKind::Extract => "extract",
        JobKind::Embed => "embed",
        JobKind::Ingest => "ingest",
    }
}

fn format_subject(job: &ServiceJob) -> String {
    match (
        job.url.as_deref(),
        job.source_type.as_deref(),
        job.target.as_deref(),
        job.urls_json.as_ref(),
    ) {
        (Some(url), _, _, _) => url.to_string(),
        (None, Some(st), Some(tgt), _) => format!("{st}: {tgt}"),
        (None, _, Some(tgt), _) => tgt.to_string(),
        (None, _, _, Some(urls)) => format_urls_subject(urls).unwrap_or_else(|| job.id.to_string()),
        _ => job.id.to_string(),
    }
}

fn format_urls_subject(urls: &serde_json::Value) -> Option<String> {
    let arr = urls.as_array()?;
    let count = arr.len();
    let first = arr.first().and_then(serde_json::Value::as_str);
    match (count, first) {
        (0, _) => None,
        (1, Some(url)) => Some(url.to_string()),
        (_, Some(url)) => Some(format!("{url} (+{} more)", count - 1)),
        (n, _) => Some(format!("{n} URLs")),
    }
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
