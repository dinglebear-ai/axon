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
    if !args.allow_live_network {
        bail!(
            "live source benchmarking fetches and indexes an external site; rerun with --allow-live-network"
        );
    }
    if args.runs == 0 {
        bail!("--runs must be greater than zero");
    }
    let parsed_url = reqwest::Url::parse(&args.url).context("benchmark URL is invalid")?;
    if !matches!(parsed_url.scheme(), "http" | "https") {
        bail!("benchmark URL must use http or https");
    }

    support::load_dotenv_if_present()?;
    let qdrant_url = support::resolve_url("QDRANT_URL", args.qdrant_url.clone())
        .or_else(|| std::env::var("AXON_QDRANT_URL").ok())
        .ok_or_else(|| anyhow!("QDRANT_URL or AXON_QDRANT_URL is required"))?;
    let tei_url = support::resolve_url("TEI_URL", args.tei_url.clone());
    let axon_bin = args.axon_bin.clone().unwrap_or_else(|| {
        support::default_axon_bin(root).unwrap_or_else(|| PathBuf::from("axon"))
    });
    let axon_version = command_text(Command::new(&axon_bin).arg("--version"))?;
    let git_sha = command_text(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(root),
    )?;
    let git_branch = command_text(
        Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(root),
    )?;

    let work_root = root.join("target/bench-source");
    fs::create_dir_all(&work_root)?;
    let stamp = format!("{}-{}", support::unix_timestamp(), std::process::id());
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| work_root.join(format!("results-{stamp}.json")));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let tei_metrics_url =
        support::select_tei_metrics_url(&client, tei_url.as_deref(), tei_url.is_some());
    let scenarios = match args.scenario {
        ScenarioMode::Cold => vec![false],
        ScenarioMode::Warm => vec![true],
        ScenarioMode::Both => vec![false, true],
    };
    let mut records = Vec::new();

    for warm in scenarios {
        let name = if warm { "warm" } else { "cold" };
        let shared_state = work_root.join(format!("state-{stamp}-{name}"));
        let shared_collection = format!("axon_source_bench_{}_{name}", stamp.replace('-', "_"));
        if warm {
            fs::create_dir_all(&shared_state)?;
            eprintln!("priming warm benchmark cache for {}", args.url);
            execute_run(
                &client,
                &axon_bin,
                &args.url,
                true,
                0,
                args.max_pages,
                &shared_state,
                &shared_collection,
                &qdrant_url,
                tei_metrics_url.as_deref(),
            )?;
        }
        for iteration in 1..=args.runs {
            let state = if warm {
                shared_state.clone()
            } else {
                work_root.join(format!("state-{stamp}-{name}-{iteration}"))
            };
            let collection = if warm {
                shared_collection.clone()
            } else {
                format!(
                    "axon_source_bench_{}_{name}_{iteration}",
                    stamp.replace('-', "_")
                )
            };
            fs::create_dir_all(&state)?;
            eprintln!("running {name} benchmark {iteration}/{}", args.runs);
            let record = execute_run(
                &client,
                &axon_bin,
                &args.url,
                warm,
                iteration,
                args.max_pages,
                &state,
                &collection,
                &qdrant_url,
                tei_metrics_url.as_deref(),
            )?;
            records.push(record);
            if !args.keep_state && !warm {
                cleanup_generated_state(&work_root, &state)?;
                support::delete_collection(&client, &qdrant_url, &collection)
                    .with_context(|| format!("remove benchmark collection {collection}"))?;
            }
        }
        if !args.keep_state && warm {
            cleanup_generated_state(&work_root, &shared_state)?;
            support::delete_collection(&client, &qdrant_url, &shared_collection)
                .with_context(|| format!("remove benchmark collection {shared_collection}"))?;
        }
    }

    finish_report(args, output, git_sha, git_branch, axon_version, records)
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
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summarizes_phase_wall_time_terminal_counts_and_warning_codes() {
        let page = json!({
            "events": [
                {
                    "sequence": 1,
                    "phase": "discovering",
                    "status": "running",
                    "timestamp": "2026-08-11T22:45:03Z",
                    "details": {"source_progress_event": {"counts": {"items_done": 0, "documents_done": 0, "chunks_done": 0}}}
                },
                {
                    "sequence": 2,
                    "phase": "discovering",
                    "status": "completed",
                    "timestamp": "2026-08-11T22:45:05Z",
                    "details": {"source_progress_event": {"counts": {"items_done": 370, "items_total": 370, "documents_done": 0, "chunks_done": 0}}}
                },
                {
                    "sequence": 3,
                    "phase": "embedding",
                    "status": "running",
                    "timestamp": "2026-08-11T22:45:10Z",
                    "details": {"source_progress_event": {"counts": {"items_done": 370, "documents_done": 64, "chunks_done": 512}}}
                },
                {
                    "sequence": 4,
                    "phase": "embedding",
                    "status": "completed",
                    "timestamp": "2026-08-11T22:45:16Z",
                    "details": {"source_progress_event": {"counts": {"items_done": 370, "documents_done": 370, "chunks_done": 7904}}}
                },
                {
                    "sequence": 5,
                    "phase": "publishing",
                    "status": "completed_degraded",
                    "timestamp": "2026-08-11T22:45:17Z",
                    "details": {"source_progress_event": {
                        "counts": {"items_done": 370, "documents_done": 370, "chunks_done": 7904},
                        "warning": {"code": "source.vectorize.redaction_skipped_chunks"}
                    }}
                }
            ]
        });

        let summary = summarize_events(&page).expect("valid event summary");
        assert_eq!(summary.phase_seconds["discovering"], 2.0);
        assert_eq!(summary.phase_seconds["embedding"], 6.0);
        assert_eq!(summary.items, 370);
        assert_eq!(summary.documents, 370);
        assert_eq!(summary.chunks, 7904);
        assert_eq!(
            summary.warning_codes["source.vectorize.redaction_skipped_chunks"],
            1
        );
    }

    #[test]
    fn repeated_phase_timings_exclude_time_spent_in_other_phases() {
        let page = json!({"events": [
            {"sequence": 1, "phase": "embedding", "status": "running", "timestamp": "2026-08-11T22:00:00Z", "details": {"source_progress_event": {"counts": {}}}},
            {"sequence": 2, "phase": "embedding", "status": "completed", "timestamp": "2026-08-11T22:00:02Z", "details": {"source_progress_event": {"counts": {}}}},
            {"sequence": 3, "phase": "upserting", "status": "running", "timestamp": "2026-08-11T22:00:02Z", "details": {"source_progress_event": {"counts": {}}}},
            {"sequence": 4, "phase": "upserting", "status": "completed", "timestamp": "2026-08-11T22:00:12Z", "details": {"source_progress_event": {"counts": {}}}},
            {"sequence": 5, "phase": "embedding", "status": "running", "timestamp": "2026-08-11T22:00:12Z", "details": {"source_progress_event": {"counts": {}}}},
            {"sequence": 6, "phase": "embedding", "status": "completed", "timestamp": "2026-08-11T22:00:15Z", "details": {"source_progress_event": {"counts": {}}}}
        ]});

        let summary = summarize_events(&page).unwrap();
        assert_eq!(summary.phase_seconds["embedding"], 5.0);
        assert_eq!(summary.phase_seconds["upserting"], 10.0);
    }

    #[test]
    fn distribution_is_stable_for_even_and_odd_sample_counts() {
        assert_eq!(
            distribution(&[9.0, 1.0, 5.0]).unwrap(),
            Distribution {
                min: 1.0,
                median: 5.0,
                max: 9.0
            }
        );
        assert_eq!(distribution(&[8.0, 2.0]).unwrap().median, 5.0);
        assert!(distribution(&[]).is_err());
    }

    #[test]
    fn percent_change_reports_improvement_and_handles_zero_baseline() {
        assert_eq!(percent_change(100.0, 75.0), Some(-25.0));
        assert_eq!(percent_change(100.0, 125.0), Some(25.0));
        assert_eq!(percent_change(0.0, 5.0), None);
    }

    #[test]
    fn finds_job_id_across_supported_command_projections() {
        assert_eq!(
            find_job_id(&json!({"job_id": "job-direct"})),
            Some("job-direct")
        );
        assert_eq!(
            find_job_id(&json!({"result": {"summary": {"job_id": "job-nested"}}})),
            Some("job-nested")
        );
        assert_eq!(find_job_id(&json!({"status": "completed"})), None);
    }

    #[test]
    fn warm_scenario_is_explicitly_cached_and_conditional() {
        assert_eq!(source_cache_args(false), vec!["--cache", "false"]);
        assert_eq!(
            source_cache_args(true),
            vec!["--cache", "true", "--etag-conditional"]
        );
    }

    #[test]
    fn baseline_must_contain_every_candidate_scenario() {
        let summary = |scenario: &str| ScenarioSummary {
            scenario: scenario.to_string(),
            runs: 1,
            wall_seconds: Distribution {
                min: 1.0,
                median: 1.0,
                max: 1.0,
            },
            items: Distribution {
                min: 1.0,
                median: 1.0,
                max: 1.0,
            },
            documents: Distribution {
                min: 1.0,
                median: 1.0,
                max: 1.0,
            },
            chunks: Distribution {
                min: 1.0,
                median: 1.0,
                max: 1.0,
            },
            points: None,
            phase_median_seconds: BTreeMap::new(),
            unattributed_seconds: Distribution {
                min: 0.0,
                median: 0.0,
                max: 0.0,
            },
            degraded_runs: 0,
            warning_codes: BTreeMap::new(),
            retry_events: 0,
        };
        let baseline = vec![summary("cold")];
        let candidate = vec![summary("cold"), summary("warm")];

        let error = validate_baseline_scenarios(&baseline, &candidate).unwrap_err();
        assert!(error.to_string().contains("warm"));
    }
}
