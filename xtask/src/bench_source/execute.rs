use super::*;

pub(super) fn execute_run(
    client: &reqwest::blocking::Client,
    axon_bin: &Path,
    url: &str,
    warm: bool,
    iteration: usize,
    max_pages: Option<u64>,
    state: &Path,
    collection: &str,
    qdrant_url: &str,
    tei_metrics_url: Option<&str>,
) -> anyhow::Result<RunRecord> {
    let tei_before = tei_metrics_url.and_then(|url| support::fetch_tei_metrics(client, url).ok());
    let qdrant_before = support::fetch_qdrant_metrics(client, qdrant_url).ok();
    let mut command = Command::new(axon_bin);
    command
        .arg("source")
        .arg(url)
        .args(["--scope", "site", "--wait", "true", "--json", "--quiet"])
        .args(source_cache_args(warm))
        .args(["--collection", collection])
        .env("AXON_HOME", state)
        .env("AXON_DATA_DIR", state)
        .env("QDRANT_URL", qdrant_url)
        .env("AXON_QDRANT_URL", qdrant_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(max_pages) = max_pages {
        command.args(["--max-pages", &max_pages.to_string()]);
    }
    if let Some(tei_url) = tei_metrics_url {
        command.env("TEI_URL", tei_url);
    }
    let started = Instant::now();
    let output = command
        .output()
        .with_context(|| format!("run {}", axon_bin.display()))?;
    let wall_seconds = started.elapsed().as_secs_f64();
    let source_json = successful_json(output, "axon source")?;
    let job_id = find_job_id(&source_json)
        .context("axon source JSON did not contain a job_id")?
        .to_string();
    let events = load_all_events(axon_bin, state, &job_id)?;
    let event_summary = summarize_events(&events)?;
    let job = command_json(
        Command::new(axon_bin)
            .args(["jobs", "get", &job_id, "--json", "--quiet"])
            .env("AXON_HOME", state)
            .env("AXON_DATA_DIR", state),
        "axon jobs get",
    )?;
    let status = find_string_key(&job, "status")
        .unwrap_or("unknown")
        .to_string();
    let points = support::fetch_collection_info(client, qdrant_url, collection)
        .ok()
        .and_then(|value| {
            value
                .pointer("/result/points_count")
                .and_then(Value::as_u64)
        });
    let tei_after = tei_metrics_url.and_then(|url| support::fetch_tei_metrics(client, url).ok());
    let qdrant_after = support::fetch_qdrant_metrics(client, qdrant_url).ok();
    let attributed_seconds = event_summary.phase_seconds.values().sum::<f64>();
    Ok(RunRecord {
        scenario: if warm { "warm" } else { "cold" }.to_string(),
        iteration,
        job_id,
        status,
        wall_seconds,
        unattributed_seconds: (wall_seconds - attributed_seconds).max(0.0),
        phase_seconds: event_summary.phase_seconds,
        items: event_summary.items,
        documents: event_summary.documents,
        chunks: event_summary.chunks,
        points,
        warning_codes: event_summary.warning_codes,
        retry_events: event_summary.retry_events,
        tei_inputs: support::metric_delta(&tei_before, &tei_after, "te_embed_count"),
        tei_requests: support::metric_delta(
            &tei_before,
            &tei_after,
            r#"te_request_success{method="batch"}"#,
        ),
        tei_input_tokens: support::metric_delta(
            &tei_before,
            &tei_after,
            "te_request_input_length_sum",
        ),
        tei_embed_seconds: support::metric_delta(&tei_before, &tei_after, "te_embed_duration_sum"),
        tei_queue_seconds: support::metric_delta(
            &tei_before,
            &tei_after,
            "te_embed_queue_duration_sum",
        ),
        qdrant_upsert_requests: support::metric_delta(
            &qdrant_before,
            &qdrant_after,
            r#"rest_responses_total{method="PUT",endpoint="/collections/{collection_name}/points",status="200"}"#,
        ),
        qdrant_upsert_seconds: support::metric_delta_by_prefix(
            &qdrant_before,
            &qdrant_after,
            r#"rest_responses_duration_seconds_sum{method="PUT",endpoint="/collections/{collection_name}/points""#,
        ),
    })
}

fn load_all_events(axon_bin: &Path, state: &Path, job_id: &str) -> anyhow::Result<Value> {
    let mut after = 0_u64;
    let mut events = Vec::new();
    loop {
        let page = command_json(
            Command::new(axon_bin)
                .args([
                    "jobs",
                    "events",
                    job_id,
                    "--after-sequence",
                    &after.to_string(),
                    "--limit",
                    "1000",
                    "--json",
                    "--quiet",
                ])
                .env("AXON_HOME", state)
                .env("AXON_DATA_DIR", state),
            "axon jobs events",
        )?;
        let page_events = page
            .get("events")
            .and_then(Value::as_array)
            .context("job events page has no events array")?;
        let count = page_events.len();
        events.extend(page_events.iter().cloned());
        let next = page
            .get("last_sequence")
            .and_then(Value::as_u64)
            .unwrap_or(after);
        if count < 1000 || next <= after {
            break;
        }
        after = next;
    }
    Ok(serde_json::json!({"events": events}))
}

fn successful_json(output: std::process::Output, label: &str) -> anyhow::Result<Value> {
    if !output.status.success() {
        bail!(
            "{label} failed with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "{label} did not emit valid JSON; stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn command_json(command: &mut Command, label: &str) -> anyhow::Result<Value> {
    successful_json(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?,
        label,
    )
}
