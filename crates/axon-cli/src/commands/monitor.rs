use axon_core::config::Config;
use axon_services::context::ServiceContext;
use axon_services::system::{StatusJobs, load_status_jobs};
use axon_services::types::ServiceJob;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobMonitorState {
    #[serde(default)]
    initialized: bool,
    #[serde(default)]
    monitor_started_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    statuses: HashMap<String, String>,
}

impl JobMonitorState {
    pub fn remember(&mut self, kind: &str, id: Uuid, status: &str) {
        self.statuses
            .insert(state_key(kind, id), status.to_string());
    }

    pub fn status_of(&self, kind: &str, id: Uuid) -> Option<&str> {
        self.statuses.get(&state_key(kind, id)).map(String::as_str)
    }

    pub fn mark_monitor_started_at(&mut self, timestamp: chrono::DateTime<chrono::Utc>) {
        self.monitor_started_at.get_or_insert(timestamp);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct JobMonitorEvent {
    pub event: &'static str,
    pub kind: String,
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunks: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed_job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MonitorOptions {
    watch: bool,
    jsonl: bool,
    interval_secs: u64,
    state_file: PathBuf,
}

pub async fn run_monitor(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let options = MonitorOptions::from_positional(&cfg.positional)?;
    let mut state = read_state(&options.state_file);
    state.mark_monitor_started_at(chrono::Utc::now());

    loop {
        let (jobs, errors) = match load_monitor_status_jobs(service_context).await {
            Ok(result) => result,
            Err(err) if options.watch => {
                eprintln!("monitor status failed: {err}; retrying");
                tokio::time::sleep(Duration::from_secs(options.interval_secs.max(1))).await;
                continue;
            }
            Err(err) => return Err(err),
        };
        for error in errors {
            eprintln!("monitor status degraded: {error}");
        }

        let events = detect_events_from_status_jobs(&mut state, &jobs);
        write_state(&options.state_file, &state)?;
        emit_events(&events, options.jsonl)?;

        if !options.watch {
            break;
        }

        tokio::time::sleep(Duration::from_secs(options.interval_secs.max(1))).await;
    }

    Ok(())
}

async fn load_monitor_status_jobs(
    service_context: &ServiceContext,
) -> Result<(StatusJobs, Vec<String>), Box<dyn Error>> {
    let (jobs, _totals, errors) = load_status_jobs(service_context).await?;
    Ok((jobs, errors))
}

pub fn detect_job_events(
    state: &mut JobMonitorState,
    crawl_jobs: &[(&str, ServiceJob)],
    embed_jobs: &[(&str, ServiceJob)],
    operation_jobs: &[(&str, ServiceJob)],
) -> Vec<JobMonitorEvent> {
    let mut events = Vec::new();
    for (kind, job) in crawl_jobs
        .iter()
        .chain(embed_jobs.iter())
        .chain(operation_jobs.iter())
    {
        if let Some(event) = detect_one(state, kind, job) {
            events.push(event);
        }
    }
    state.initialized = true;
    events
}

fn detect_events_from_status_jobs(
    state: &mut JobMonitorState,
    jobs: &StatusJobs,
) -> Vec<JobMonitorEvent> {
    let crawl: Vec<_> = jobs
        .crawl
        .iter()
        .cloned()
        .map(|job| ("crawl", job))
        .collect();
    let embed: Vec<_> = jobs
        .embed
        .iter()
        .cloned()
        .map(|job| ("embed", job))
        .collect();
    let operations: Vec<_> = jobs
        .extract
        .iter()
        .cloned()
        .map(|job| ("extract", job))
        .chain(jobs.ingest.iter().cloned().map(|job| ("ingest", job)))
        .collect();

    detect_job_events(state, &crawl, &embed, &operations)
}

fn detect_one(
    state: &mut JobMonitorState,
    kind: &str,
    job: &ServiceJob,
) -> Option<JobMonitorEvent> {
    let previous = state.status_of(kind, job.id).map(str::to_string);
    state.remember(kind, job.id, &job.status);

    let event = match (previous.as_deref(), job.status.as_str()) {
        (Some("running"), "completed") | (Some("pending"), "completed") => "completed",
        (Some("running"), "failed") | (Some("pending"), "failed") => "failed",
        (Some("running"), "canceled") | (Some("pending"), "canceled") => "canceled",
        (None, "completed") if state.initialized || job_started_after_monitor(state, job) => {
            "completed"
        }
        (None, "failed") if state.initialized || job_started_after_monitor(state, job) => "failed",
        (None, "canceled") if state.initialized || job_started_after_monitor(state, job) => {
            "canceled"
        }
        (prev, "running") if prev != Some("running") => "started",
        _ => return None,
    };

    Some(JobMonitorEvent {
        event,
        kind: kind.to_string(),
        id: job.id,
        target: job_target(job),
        status: job.status.clone(),
        docs: metric_u64(job.result_json.as_ref(), &docs_metric_keys(kind)),
        chunks: metric_u64(
            job.result_json.as_ref(),
            &["chunks_embedded", "chunks", "chunk_count"],
        ),
        embed_job_id: metric_string(job.result_json.as_ref(), &["embed_job_id", "embed_job"]),
        error: job.error_text.clone(),
        created_at: job.created_at,
        updated_at: job.updated_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
    })
}

fn job_started_after_monitor(state: &JobMonitorState, job: &ServiceJob) -> bool {
    state
        .monitor_started_at
        .is_some_and(|started_at| job.created_at >= started_at)
}

fn docs_metric_keys(kind: &str) -> [&'static str; 5] {
    match kind {
        "crawl" => [
            "docs",
            "pages_crawled",
            "md_created",
            "pages_visited",
            "documents",
        ],
        "extract" => [
            "items",
            "documents",
            "docs",
            "pages_visited",
            "pages_crawled",
        ],
        _ => [
            "docs_embedded",
            "docs_completed",
            "documents",
            "docs",
            "files_done",
        ],
    }
}

fn metric_u64(result_json: Option<&Value>, keys: &[&str]) -> Option<u64> {
    let result_json = result_json?;
    keys.iter()
        .find_map(|key| result_json.get(*key).and_then(Value::as_u64))
}

fn metric_string(result_json: Option<&Value>, keys: &[&str]) -> Option<String> {
    let result_json = result_json?;
    keys.iter().find_map(|key| {
        result_json
            .get(*key)
            .and_then(Value::as_str)
            .map(ToString::to_string)
    })
}

fn job_target(job: &ServiceJob) -> Option<String> {
    if let Some(url) = job.url.as_ref().filter(|url| !url.is_empty()) {
        return Some(url.clone());
    }

    if let Some(target) = job.target.as_ref().filter(|target| !target.is_empty()) {
        if let Some(source_type) = job.source_type.as_ref().filter(|source| !source.is_empty()) {
            if target.starts_with(&format!("{source_type}:")) {
                return Some(target.clone());
            }
            return Some(format!("{source_type}:{target}"));
        }
        return Some(target.clone());
    }

    job.urls_json
        .as_ref()
        .and_then(Value::as_array)
        .and_then(|urls| urls.first())
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

impl MonitorOptions {
    fn from_positional(positional: &[String]) -> Result<Self, Box<dyn Error>> {
        if positional.first().map(String::as_str) != Some("jobs") {
            return Err("monitor requires a subcommand: jobs".into());
        }

        let mut watch = false;
        let mut jsonl = false;
        let mut interval_secs = 5;
        let mut state_file = None;
        let mut idx = 1;

        while idx < positional.len() {
            match positional[idx].as_str() {
                "--watch" => watch = true,
                "--jsonl" => jsonl = true,
                "--interval-secs" => {
                    idx += 1;
                    let raw = positional
                        .get(idx)
                        .ok_or("--interval-secs requires a value")?;
                    interval_secs = raw.parse::<u64>()?;
                }
                "--state-file" => {
                    idx += 1;
                    let raw = positional.get(idx).ok_or("--state-file requires a value")?;
                    state_file = Some(PathBuf::from(raw));
                }
                other => return Err(format!("unknown monitor jobs option: {other}").into()),
            }
            idx += 1;
        }

        Ok(Self {
            watch,
            jsonl,
            interval_secs,
            state_file: state_file.unwrap_or_else(default_state_file),
        })
    }
}

fn default_state_file() -> PathBuf {
    axon_core::paths::axon_data_base_dir()
        .join("monitor")
        .join("jobs-state.json")
}

fn read_state(path: &PathBuf) -> JobMonitorState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_state(path: &PathBuf, state: &JobMonitorState) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp_path, serde_json::to_vec_pretty(state)?)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

fn emit_events(events: &[JobMonitorEvent], jsonl: bool) -> Result<(), Box<dyn Error>> {
    let mut stdout = std::io::stdout().lock();
    for event in events {
        if jsonl {
            writeln!(stdout, "{}", serde_json::to_string(event)?)?;
        } else {
            writeln!(
                stdout,
                "{} {} {} {}",
                event.kind,
                event.event,
                event.id,
                event.target.as_deref().unwrap_or("")
            )?;
        }
    }
    stdout.flush()?;
    Ok(())
}

fn state_key(kind: &str, id: Uuid) -> String {
    format!("{kind}:{id}")
}
