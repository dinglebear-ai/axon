use crate::bench_embed::support;
use anyhow::{Context, anyhow, bail};
use chrono::DateTime;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

mod execute;
use execute::execute_run;
mod report;
mod runner;
use report::{compare_summaries, print_human, summarize_records};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct EventSummary {
    phase_seconds: BTreeMap<String, f64>,
    items: u64,
    documents: u64,
    chunks: u64,
    warning_codes: BTreeMap<String, u64>,
    retry_events: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Distribution {
    min: f64,
    median: f64,
    max: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ScenarioMode {
    Cold,
    Warm,
    Both,
}

#[derive(Debug, Clone)]
pub struct BenchSourceArgs {
    pub url: String,
    pub axon_bin: Option<PathBuf>,
    pub scenario: ScenarioMode,
    pub runs: usize,
    pub max_pages: Option<u64>,
    pub qdrant_url: Option<String>,
    pub tei_url: Option<String>,
    pub output: Option<PathBuf>,
    pub baseline: Option<PathBuf>,
    pub keep_state: bool,
    pub allow_live_network: bool,
    pub json: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunRecord {
    scenario: String,
    iteration: usize,
    job_id: String,
    status: String,
    wall_seconds: f64,
    unattributed_seconds: f64,
    phase_seconds: BTreeMap<String, f64>,
    items: u64,
    documents: u64,
    chunks: u64,
    points: Option<u64>,
    warning_codes: BTreeMap<String, u64>,
    retry_events: u64,
    tei_inputs: Option<f64>,
    tei_requests: Option<f64>,
    tei_input_tokens: Option<f64>,
    tei_embed_seconds: Option<f64>,
    tei_queue_seconds: Option<f64>,
    qdrant_upsert_requests: Option<f64>,
    qdrant_upsert_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScenarioSummary {
    scenario: String,
    runs: usize,
    wall_seconds: Distribution,
    items: Distribution,
    documents: Distribution,
    chunks: Distribution,
    points: Option<Distribution>,
    phase_median_seconds: BTreeMap<String, f64>,
    unattributed_seconds: Distribution,
    degraded_runs: usize,
    warning_codes: BTreeMap<String, u64>,
    retry_events: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScenarioComparison {
    scenario: String,
    wall_median_percent: Option<f64>,
    items_median_percent: Option<f64>,
    documents_median_percent: Option<f64>,
    chunks_median_percent: Option<f64>,
    points_median_percent: Option<f64>,
    phase_median_percent: BTreeMap<String, f64>,
    degraded_runs_delta: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchReport {
    schema: String,
    timestamp_utc: String,
    url: String,
    git_sha: String,
    git_branch: String,
    axon_version: String,
    scenario: String,
    runs_per_scenario: usize,
    max_pages: Option<u64>,
    tuning: BTreeMap<String, String>,
    service_metrics_scope: String,
    records: Vec<RunRecord>,
    summaries: Vec<ScenarioSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    comparison: Vec<ScenarioComparison>,
}

pub fn run(root: &Path, args: BenchSourceArgs) -> anyhow::Result<()> {
    runner::run(root, args)
}

fn finish_report(
    args: BenchSourceArgs,
    output: PathBuf,
    git_sha: String,
    git_branch: String,
    axon_version: String,
    records: Vec<RunRecord>,
) -> anyhow::Result<()> {
    let summaries = summarize_records(&records)?;
    let comparison = if let Some(path) = &args.baseline {
        let baseline: BenchReport = serde_json::from_slice(
            &fs::read(path).with_context(|| format!("read baseline {}", path.display()))?,
        )?;
        validate_baseline(&baseline, &args.url, args.max_pages)?;
        validate_baseline_scenarios(&baseline.summaries, &summaries)?;
        compare_summaries(&baseline.summaries, &summaries)
    } else {
        Vec::new()
    };
    let report = BenchReport {
        schema: "axon-bench-source/v1".to_string(),
        timestamp_utc: chrono::Utc::now().to_rfc3339(),
        url: args.url,
        git_sha,
        git_branch,
        axon_version,
        scenario: format!("{:?}", args.scenario).to_ascii_lowercase(),
        runs_per_scenario: args.runs,
        max_pages: args.max_pages,
        tuning: benchmark_tuning(),
        service_metrics_scope: "process-wide Prometheus deltas; use otherwise-idle TEI and Qdrant services for uncontaminated provider metrics".to_string(),
        records,
        summaries,
        comparison,
    };
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&output, format!("{json}\n"))?;
    if args.json {
        println!("{json}");
    } else {
        print_human(&report, &output);
    }
    Ok(())
}

fn command_text(command: &mut Command) -> anyhow::Result<String> {
    let output = command.output()?;
    if !output.status.success() {
        bail!("command failed with {}", output.status);
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn find_string_key<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    match value {
        Value::Object(map) => map
            .get(key)
            .and_then(Value::as_str)
            .or_else(|| map.values().find_map(|value| find_string_key(value, key))),
        Value::Array(values) => values.iter().find_map(|value| find_string_key(value, key)),
        _ => None,
    }
}

fn cleanup_generated_state(root: &Path, path: &Path) -> anyhow::Result<()> {
    let root = root.canonicalize()?;
    let path = path.canonicalize()?;
    if path.parent() != Some(root.as_path())
        || !path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("state-"))
    {
        bail!(
            "refusing to remove non-benchmark state directory {}",
            path.display()
        );
    }
    fs::remove_dir_all(path)?;
    Ok(())
}

fn cleanup_benchmark_resources(
    client: &reqwest::blocking::Client,
    work_root: &Path,
    qdrant_url: &str,
    state: &Path,
    collection: &str,
) -> anyhow::Result<()> {
    let state_cleanup = cleanup_generated_state(work_root, state);
    let collection_cleanup = support::delete_collection(client, qdrant_url, collection)
        .with_context(|| format!("remove benchmark collection {collection}"));
    match (state_cleanup, collection_cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(state_error), Err(collection_error)) => Err(anyhow!(
            "benchmark state cleanup failed: {state_error:#}; collection cleanup failed: {collection_error:#}"
        )),
    }
}

fn combine_run_and_cleanup(error: anyhow::Error, cleanup: anyhow::Result<()>) -> anyhow::Error {
    match cleanup {
        Ok(()) => error,
        Err(cleanup_error) => anyhow!(
            "benchmark run failed: {error:#}; benchmark cleanup also failed: {cleanup_error:#}"
        ),
    }
}

fn resolve_run_with_cleanup<T>(
    run: anyhow::Result<T>,
    cleanup: anyhow::Result<()>,
) -> anyhow::Result<T> {
    match (run, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Err(error), Err(cleanup_error)) => Err(anyhow!(
            "benchmark run failed: {error:#}; benchmark cleanup also failed: {cleanup_error:#}"
        )),
    }
}

fn validate_baseline(
    baseline: &BenchReport,
    url: &str,
    max_pages: Option<u64>,
) -> anyhow::Result<()> {
    if baseline.schema != "axon-bench-source/v1" {
        bail!("baseline has unsupported schema {}", baseline.schema);
    }
    if baseline.url != url {
        bail!(
            "baseline URL {} does not match candidate URL {url}",
            baseline.url
        );
    }
    if baseline.max_pages != max_pages {
        bail!(
            "baseline max_pages {:?} does not match candidate max_pages {:?}",
            baseline.max_pages,
            max_pages
        );
    }
    Ok(())
}

fn validate_baseline_scenarios(
    baseline: &[ScenarioSummary],
    candidate: &[ScenarioSummary],
) -> anyhow::Result<()> {
    for scenario in candidate {
        if !baseline
            .iter()
            .any(|prior| prior.scenario == scenario.scenario)
        {
            bail!(
                "baseline does not contain candidate scenario {}",
                scenario.scenario
            );
        }
    }
    Ok(())
}

fn benchmark_tuning() -> BTreeMap<String, String> {
    const KEYS: &[(&str, &str)] = &[
        ("TEI_MAX_CLIENT_BATCH_SIZE", "96"),
        ("AXON_TEI_MAX_CONCURRENT", "8"),
        ("AXON_TEI_MAX_IN_FLIGHT_INPUTS", "320"),
        ("AXON_EMBED_PREP_CONCURRENCY", "auto"),
    ];
    KEYS.iter()
        .map(|(key, default)| {
            (
                (*key).to_string(),
                std::env::var(key).unwrap_or_else(|_| (*default).to_string()),
            )
        })
        .collect()
}

fn summarize_events(page: &Value) -> anyhow::Result<EventSummary> {
    let events = page
        .get("events")
        .and_then(Value::as_array)
        .context("job event response has no events array")?;
    let mut phase_seconds = BTreeMap::<String, f64>::new();
    let mut timeline = Vec::with_capacity(events.len());
    let mut items = 0;
    let mut documents = 0;
    let mut chunks = 0;
    let mut warning_codes = BTreeMap::new();
    let mut retry_events = 0;
    for event in events {
        let phase = event
            .get("phase")
            .and_then(Value::as_str)
            .context("job event has no phase")?;
        let timestamp = event
            .get("timestamp")
            .and_then(Value::as_str)
            .context("job event has no timestamp")?;
        let timestamp = DateTime::parse_from_rfc3339(timestamp)
            .with_context(|| format!("invalid job event timestamp {timestamp:?}"))?;
        let status = event
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        timeline.push((phase.to_string(), status.to_string(), timestamp));

        let progress = event.pointer("/details/source_progress_event");
        if let Some(counts) = progress.and_then(|value| value.get("counts")) {
            items = items.max(
                counts
                    .get("items_done")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            );
            documents = documents.max(
                counts
                    .get("documents_done")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            );
            chunks = chunks.max(
                counts
                    .get("chunks_done")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            );
        }
        if let Some(code) = progress
            .and_then(|value| value.pointer("/warning/code"))
            .and_then(Value::as_str)
        {
            *warning_codes.entry(code.to_string()).or_insert(0) += 1;
        }
        if progress
            .and_then(|value| value.get("retry"))
            .is_some_and(|value| !value.is_null())
        {
            retry_events += 1;
        }
    }
    for pair in timeline.windows(2) {
        let (phase, status, started) = &pair[0];
        let (_, _, finished) = &pair[1];
        if status == "running" {
            let seconds = (*finished - *started)
                .num_microseconds()
                .unwrap_or_default() as f64
                / 1_000_000.0;
            *phase_seconds.entry(phase.clone()).or_default() += seconds;
        }
    }
    Ok(EventSummary {
        phase_seconds,
        items,
        documents,
        chunks,
        warning_codes,
        retry_events,
    })
}

fn distribution(values: &[f64]) -> anyhow::Result<Distribution> {
    if values.is_empty() {
        bail!("cannot summarize an empty sample set");
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(anyhow!("sample set contains a non-finite value"));
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    let median = if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    };
    Ok(Distribution {
        min: sorted[0],
        median,
        max: sorted[sorted.len() - 1],
    })
}

fn percent_change(baseline: f64, candidate: f64) -> Option<f64> {
    (baseline != 0.0 && baseline.is_finite() && candidate.is_finite())
        .then_some((candidate - baseline) / baseline * 100.0)
}

fn find_job_id(value: &Value) -> Option<&str> {
    match value {
        Value::Object(map) => map
            .get("job_id")
            .and_then(Value::as_str)
            .or_else(|| map.values().find_map(find_job_id)),
        Value::Array(values) => values.iter().find_map(find_job_id),
        _ => None,
    }
}

fn source_cache_args(warm: bool) -> Vec<&'static str> {
    if warm {
        vec!["--cache", "true", "--etag-conditional"]
    } else {
        vec!["--cache", "false"]
    }
}

#[cfg(test)]
mod tests;
