use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

mod support;
use support::*;

#[derive(Debug, Clone)]
pub struct BenchEmbedArgs {
    pub corpus: PathBuf,
    pub axon_bin: Option<PathBuf>,
    pub collection: Option<String>,
    pub qdrant_url: Option<String>,
    pub tei_url: Option<String>,
    pub keep_collection: bool,
    pub json: bool,
}

#[derive(Debug, serde::Serialize)]
struct BenchReport {
    corpus: String,
    corpus_file_count: Option<u64>,
    collection: String,
    wall_seconds: f64,
    docs_embedded: Option<u64>,
    chunks_embedded: Option<u64>,
    points_count: Option<u64>,
    indexed_vectors_count: Option<u64>,
    segments_count: Option<u64>,
    optimizer_status: Option<String>,
    qdrant_hnsw_m: Option<u64>,
    qdrant_hnsw_ef_construct: Option<u64>,
    qdrant_indexing_threshold_kb: Option<u64>,
    docs_per_second: Option<f64>,
    chunks_per_second: Option<f64>,
    points_per_second: Option<f64>,
    files_per_second: Option<f64>,
    tei_embed_count_delta: Option<f64>,
    tei_request_success_delta: Option<f64>,
    tei_request_failure_overloaded_delta: Option<f64>,
    tei_avg_inputs_per_request: Option<f64>,
    tei_input_tokens_delta: Option<f64>,
    tei_input_count_delta: Option<f64>,
    tei_avg_input_tokens: Option<f64>,
    tei_input_tokens_per_second: Option<f64>,
    tei_embed_duration_seconds_delta: Option<f64>,
    tei_inference_duration_seconds_delta: Option<f64>,
    tei_queue_duration_seconds_delta: Option<f64>,
    tei_tokenization_duration_seconds_delta: Option<f64>,
    tei_avg_embed_seconds_per_input: Option<f64>,
    tei_avg_inference_seconds_per_input: Option<f64>,
    vllm_request_success_delta: Option<f64>,
    vllm_prompt_tokens_delta: Option<f64>,
    vllm_request_prompt_tokens_count_delta: Option<f64>,
    vllm_avg_prompt_tokens: Option<f64>,
    vllm_prompt_tokens_per_second: Option<f64>,
    vllm_time_to_first_token_seconds_delta: Option<f64>,
    vllm_avg_time_to_first_token_seconds: Option<f64>,
    qdrant_upsert_requests_delta: Option<f64>,
    qdrant_upsert_duration_seconds_delta: Option<f64>,
    qdrant_avg_upsert_seconds_per_request: Option<f64>,
    qdrant_index_requests_delta: Option<f64>,
    qdrant_index_duration_seconds_delta: Option<f64>,
    cleaned_up: bool,
}

pub fn run(root: &Path, args: BenchEmbedArgs) -> Result<()> {
    load_dotenv_if_present()?;

    let corpus = args.corpus;
    if !corpus.exists() {
        bail!("corpus path does not exist: {}", corpus.display());
    }
    let corpus_file_count = count_corpus_files(&corpus);

    let qdrant_url = resolve_url("QDRANT_URL", args.qdrant_url)
        .or_else(|| std::env::var("AXON_QDRANT_URL").ok())
        .ok_or_else(|| anyhow!("QDRANT_URL or AXON_QDRANT_URL is required"))?;
    let tei_url = resolve_url("TEI_URL", args.tei_url.clone());
    let collection = args
        .collection
        .unwrap_or_else(|| format!("axon_embed_bench_{}", unix_timestamp()));
    let axon_bin = args
        .axon_bin
        .unwrap_or_else(|| default_axon_bin(root).unwrap_or_else(|| PathBuf::from("axon")));

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let tei_metrics_url =
        select_tei_metrics_url(&client, tei_url.as_deref(), args.tei_url.is_some());
    let tei_before = tei_metrics_url
        .as_deref()
        .and_then(|url| fetch_tei_metrics(&client, url).ok());
    let qdrant_before = fetch_qdrant_metrics(&client, &qdrant_url).ok();

    let start = Instant::now();
    let mut command = Command::new(&axon_bin);
    command
        .arg("embed")
        .arg(&corpus)
        .arg("--wait")
        .arg("true")
        .arg("--json")
        .env("AXON_COLLECTION", &collection)
        .env("QDRANT_URL", &qdrant_url)
        .env("AXON_QDRANT_URL", &qdrant_url);
    if let Some(tei_url) = &tei_url {
        command.env("TEI_URL", tei_url);
    }
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run {}", axon_bin.display()))?;
    let wall_seconds = start.elapsed().as_secs_f64();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!(
            "axon embed failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            stdout,
            stderr
        );
    }
    let axon_json = serde_json::from_slice::<Value>(&output.stdout).ok();

    let collection_info = fetch_collection_info(&client, &qdrant_url, &collection).ok();
    let tei_after = tei_metrics_url
        .as_deref()
        .and_then(|url| fetch_tei_metrics(&client, url).ok());
    let qdrant_after = fetch_qdrant_metrics(&client, &qdrant_url).ok();

    let points_count = collection_info
        .as_ref()
        .and_then(|info| info.pointer("/result/points_count").and_then(Value::as_u64));
    let indexed_vectors_count = collection_info.as_ref().and_then(|info| {
        info.pointer("/result/indexed_vectors_count")
            .and_then(Value::as_u64)
    });
    let segments_count = collection_info.as_ref().and_then(|info| {
        info.pointer("/result/segments_count")
            .and_then(Value::as_u64)
    });
    let optimizer_status = collection_info
        .as_ref()
        .and_then(|info| {
            info.pointer("/result/optimizer_status")
                .and_then(Value::as_str)
        })
        .map(ToOwned::to_owned);
    let qdrant_hnsw_m = collection_info.as_ref().and_then(|info| {
        info.pointer("/result/config/hnsw_config/m")
            .and_then(Value::as_u64)
    });
    let qdrant_hnsw_ef_construct = collection_info.as_ref().and_then(|info| {
        info.pointer("/result/config/hnsw_config/ef_construct")
            .and_then(Value::as_u64)
    });
    let qdrant_indexing_threshold_kb = collection_info.as_ref().and_then(|info| {
        info.pointer("/result/config/optimizer_config/indexing_threshold")
            .and_then(Value::as_u64)
    });
    let docs_embedded = first_u64_pointer(
        axon_json.as_ref(),
        &[
            "/docs_embedded",
            "/summary/docs_embedded",
            "/result/docs_embedded",
        ],
    )
    .or(corpus_file_count);
    let chunks_embedded = first_u64_pointer(
        axon_json.as_ref(),
        &[
            "/chunks_embedded",
            "/summary/chunks_embedded",
            "/result/chunks_embedded",
        ],
    )
    .or(points_count);
    let docs_per_second = docs_embedded.map(|docs| docs as f64 / wall_seconds);
    let chunks_per_second = chunks_embedded.map(|chunks| chunks as f64 / wall_seconds);
    let points_per_second = points_count.map(|points| points as f64 / wall_seconds);
    let files_per_second = corpus_file_count.map(|files| files as f64 / wall_seconds);
    let tei_embed_count_delta = metric_delta(&tei_before, &tei_after, "te_embed_count");
    let tei_request_success_delta = metric_delta(
        &tei_before,
        &tei_after,
        r#"te_request_success{method="batch"}"#,
    );
    let tei_avg_inputs_per_request = match (tei_embed_count_delta, tei_request_success_delta) {
        (Some(inputs), Some(requests)) if requests > 0.0 => Some(inputs / requests),
        _ => None,
    };
    let tei_input_tokens_delta =
        metric_delta(&tei_before, &tei_after, "te_request_input_length_sum");
    let tei_input_count_delta =
        metric_delta(&tei_before, &tei_after, "te_request_input_length_count");
    let tei_avg_input_tokens = match (tei_input_tokens_delta, tei_input_count_delta) {
        (Some(tokens), Some(inputs)) if inputs > 0.0 => Some(tokens / inputs),
        _ => None,
    };
    let tei_input_tokens_per_second = tei_input_tokens_delta.map(|tokens| tokens / wall_seconds);
    let tei_embed_duration_seconds_delta =
        metric_delta(&tei_before, &tei_after, "te_embed_duration_sum");
    let tei_inference_duration_seconds_delta =
        metric_delta(&tei_before, &tei_after, "te_embed_inference_duration_sum");
    let tei_queue_duration_seconds_delta =
        metric_delta(&tei_before, &tei_after, "te_embed_queue_duration_sum");
    let tei_tokenization_duration_seconds_delta = metric_delta(
        &tei_before,
        &tei_after,
        "te_request_tokenization_duration_sum",
    );
    let tei_avg_embed_seconds_per_input =
        match (tei_embed_duration_seconds_delta, tei_embed_count_delta) {
            (Some(seconds), Some(inputs)) if inputs > 0.0 => Some(seconds / inputs),
            _ => None,
        };
    let tei_avg_inference_seconds_per_input =
        match (tei_inference_duration_seconds_delta, tei_embed_count_delta) {
            (Some(seconds), Some(inputs)) if inputs > 0.0 => Some(seconds / inputs),
            _ => None,
        };
    let vllm_request_success_delta =
        metric_delta_by_prefix(&tei_before, &tei_after, "vllm:request_success_total{");
    let vllm_prompt_tokens_delta =
        metric_delta_by_prefix(&tei_before, &tei_after, "vllm:prompt_tokens_total{");
    let vllm_request_prompt_tokens_count_delta =
        metric_delta_by_prefix(&tei_before, &tei_after, "vllm:request_prompt_tokens_count{");
    let vllm_avg_prompt_tokens = match (
        vllm_prompt_tokens_delta,
        vllm_request_prompt_tokens_count_delta,
    ) {
        (Some(tokens), Some(inputs)) if inputs > 0.0 => Some(tokens / inputs),
        _ => None,
    };
    let vllm_prompt_tokens_per_second =
        vllm_prompt_tokens_delta.map(|tokens| tokens / wall_seconds);
    let vllm_time_to_first_token_seconds_delta = metric_delta_by_prefix(
        &tei_before,
        &tei_after,
        "vllm:time_to_first_token_seconds_sum{",
    );
    let vllm_avg_time_to_first_token_seconds = match (
        vllm_time_to_first_token_seconds_delta,
        vllm_request_prompt_tokens_count_delta,
    ) {
        (Some(seconds), Some(inputs)) if inputs > 0.0 => Some(seconds / inputs),
        _ => None,
    };
    let qdrant_upsert_requests_delta = metric_delta(
        &qdrant_before,
        &qdrant_after,
        r#"rest_responses_total{method="PUT",endpoint="/collections/{name}/points",status="200"}"#,
    );
    let qdrant_upsert_duration_seconds_delta = metric_delta(
        &qdrant_before,
        &qdrant_after,
        r#"rest_responses_duration_seconds_sum{method="PUT",endpoint="/collections/{name}/points",status="200"}"#,
    );
    let qdrant_avg_upsert_seconds_per_request = match (
        qdrant_upsert_duration_seconds_delta,
        qdrant_upsert_requests_delta,
    ) {
        (Some(seconds), Some(requests)) if requests > 0.0 => Some(seconds / requests),
        _ => None,
    };
    let qdrant_index_requests_delta = metric_delta(
        &qdrant_before,
        &qdrant_after,
        r#"rest_responses_total{method="PUT",endpoint="/collections/{name}/index",status="200"}"#,
    );
    let qdrant_index_duration_seconds_delta = metric_delta(
        &qdrant_before,
        &qdrant_after,
        r#"rest_responses_duration_seconds_sum{method="PUT",endpoint="/collections/{name}/index",status="200"}"#,
    );

    let cleaned_up = if args.keep_collection {
        false
    } else {
        delete_collection(&client, &qdrant_url, &collection)?;
        true
    };

    let report = BenchReport {
        corpus: corpus.display().to_string(),
        corpus_file_count,
        collection,
        wall_seconds,
        docs_embedded,
        chunks_embedded,
        points_count,
        indexed_vectors_count,
        segments_count,
        optimizer_status,
        qdrant_hnsw_m,
        qdrant_hnsw_ef_construct,
        qdrant_indexing_threshold_kb,
        docs_per_second,
        chunks_per_second,
        points_per_second,
        files_per_second,
        tei_embed_count_delta,
        tei_request_success_delta,
        tei_request_failure_overloaded_delta: metric_delta(
            &tei_before,
            &tei_after,
            r#"te_request_failure{err="overloaded"}"#,
        ),
        tei_avg_inputs_per_request,
        tei_input_tokens_delta,
        tei_input_count_delta,
        tei_avg_input_tokens,
        tei_input_tokens_per_second,
        tei_embed_duration_seconds_delta,
        tei_inference_duration_seconds_delta,
        tei_queue_duration_seconds_delta,
        tei_tokenization_duration_seconds_delta,
        tei_avg_embed_seconds_per_input,
        tei_avg_inference_seconds_per_input,
        vllm_request_success_delta,
        vllm_prompt_tokens_delta,
        vllm_request_prompt_tokens_count_delta,
        vllm_avg_prompt_tokens,
        vllm_prompt_tokens_per_second,
        vllm_time_to_first_token_seconds_delta,
        vllm_avg_time_to_first_token_seconds,
        qdrant_upsert_requests_delta,
        qdrant_upsert_duration_seconds_delta,
        qdrant_avg_upsert_seconds_per_request,
        qdrant_index_requests_delta,
        qdrant_index_duration_seconds_delta,
        cleaned_up,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report);
    }

    Ok(())
}
