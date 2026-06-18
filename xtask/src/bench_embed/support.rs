use super::BenchReport;
use anyhow::Result;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub(super) fn load_dotenv_if_present() -> Result<()> {
    let Some(home) = std::env::var_os("HOME") else {
        return Ok(());
    };
    let path = PathBuf::from(home).join(".axon/.env");
    if !path.is_file() {
        return Ok(());
    }
    for item in dotenvy::from_path_iter(&path)? {
        let (key, value) = item?;
        if std::env::var_os(&key).is_none() {
            // SAFETY: xtask is single-threaded here and mutates process env
            // before any worker threads are spawned.
            unsafe {
                std::env::set_var(key, value);
            }
        }
    }
    Ok(())
}

pub(super) fn resolve_url(env_key: &str, explicit: Option<String>) -> Option<String> {
    explicit
        .or_else(|| std::env::var(env_key).ok())
        .filter(|value| !value.trim().is_empty())
}

pub(super) fn default_axon_bin(root: &Path) -> Option<PathBuf> {
    let candidate = root.join("target/debug/axon");
    candidate.is_file().then_some(candidate)
}

pub(super) fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn count_corpus_files(path: &Path) -> Option<u64> {
    if path.is_file() {
        return Some(1);
    }
    if !path.is_dir() {
        return None;
    }
    Some(
        WalkDir::new(path)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .count() as u64,
    )
}

pub(super) fn first_u64_pointer(value: Option<&Value>, pointers: &[&str]) -> Option<u64> {
    let value = value?;
    pointers
        .iter()
        .find_map(|pointer| value.pointer(pointer).and_then(Value::as_u64))
}

pub(super) fn fetch_collection_info(
    client: &reqwest::blocking::Client,
    qdrant_url: &str,
    collection: &str,
) -> Result<Value> {
    let url = format!(
        "{}/collections/{}",
        qdrant_url.trim_end_matches('/'),
        collection
    );
    Ok(client.get(url).send()?.error_for_status()?.json()?)
}

pub(super) fn delete_collection(
    client: &reqwest::blocking::Client,
    qdrant_url: &str,
    collection: &str,
) -> Result<()> {
    let url = format!(
        "{}/collections/{}",
        qdrant_url.trim_end_matches('/'),
        collection
    );
    client.delete(url).send()?.error_for_status()?;
    Ok(())
}

pub(super) fn fetch_tei_metrics(
    client: &reqwest::blocking::Client,
    tei_url: &str,
) -> Result<BTreeMap<String, f64>> {
    let url = format!(
        "{}/metrics",
        metrics_base_url(tei_url).trim_end_matches('/')
    );
    let body = client.get(url).send()?.error_for_status()?.text()?;
    Ok(parse_prometheus_metrics(&body))
}

fn metrics_base_url(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
}

pub(super) fn fetch_qdrant_metrics(
    client: &reqwest::blocking::Client,
    qdrant_url: &str,
) -> Result<BTreeMap<String, f64>> {
    let url = format!("{}/metrics", qdrant_url.trim_end_matches('/'));
    let body = client.get(url).send()?.error_for_status()?.text()?;
    Ok(parse_prometheus_metrics(&body))
}

pub(super) fn select_tei_metrics_url(
    client: &reqwest::blocking::Client,
    configured: Option<&str>,
    explicit: bool,
) -> Option<String> {
    if let Some(url) = configured
        && (explicit || fetch_tei_metrics(client, url).is_ok())
    {
        return Some(url.to_string());
    }

    if explicit {
        return configured.map(ToOwned::to_owned);
    }

    let port = std::env::var("TEI_HTTP_PORT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "52000".to_string());
    let fallback = format!("http://127.0.0.1:{port}");
    fetch_tei_metrics(client, &fallback).ok().map(|_| fallback)
}

fn parse_prometheus_metrics(body: &str) -> BTreeMap<String, f64> {
    let mut metrics = BTreeMap::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.rsplit_once(' ') else {
            continue;
        };
        if let Ok(value) = value.parse::<f64>() {
            metrics.insert(name.to_string(), value);
        }
    }
    metrics
}

pub(super) fn metric_delta(
    before: &Option<BTreeMap<String, f64>>,
    after: &Option<BTreeMap<String, f64>>,
    key: &str,
) -> Option<f64> {
    let before = before.as_ref()?.get(key).copied().unwrap_or(0.0);
    let after = after.as_ref()?.get(key).copied().unwrap_or(0.0);
    Some(after - before)
}

pub(super) fn metric_delta_by_prefix(
    before: &Option<BTreeMap<String, f64>>,
    after: &Option<BTreeMap<String, f64>>,
    prefix: &str,
) -> Option<f64> {
    let after = after.as_ref()?;
    let before_sum = before
        .as_ref()
        .map(|metrics| metric_sum_by_prefix(metrics, prefix))
        .unwrap_or(0.0);
    Some(metric_sum_by_prefix(after, prefix) - before_sum)
}

fn metric_sum_by_prefix(metrics: &BTreeMap<String, f64>, prefix: &str) -> f64 {
    metrics
        .iter()
        .filter_map(|(key, value)| key.starts_with(prefix).then_some(*value))
        .sum()
}

pub(super) fn print_human(report: &BenchReport) {
    println!("Embed benchmark complete");
    println!("corpus: {}", report.corpus);
    if let Some(files) = report.corpus_file_count {
        println!("corpus_file_count: {files}");
    }
    println!("collection: {}", report.collection);
    println!("wall_seconds: {:.3}", report.wall_seconds);
    if let Some(docs) = report.docs_embedded {
        println!("docs_embedded: {docs}");
    }
    if let Some(chunks) = report.chunks_embedded {
        println!("chunks_embedded: {chunks}");
    }
    if let Some(points) = report.points_count {
        println!("points_count: {points}");
    }
    if let Some(vectors) = report.indexed_vectors_count {
        println!("indexed_vectors_count: {vectors}");
    }
    if let Some(segments) = report.segments_count {
        println!("segments_count: {segments}");
    }
    if let Some(status) = &report.optimizer_status {
        println!("optimizer_status: {status}");
    }
    if let Some(m) = report.qdrant_hnsw_m {
        println!("qdrant_hnsw_m: {m}");
    }
    if let Some(ef) = report.qdrant_hnsw_ef_construct {
        println!("qdrant_hnsw_ef_construct: {ef}");
    }
    if let Some(threshold) = report.qdrant_indexing_threshold_kb {
        println!("qdrant_indexing_threshold_kb: {threshold}");
    }
    if let Some(rate) = report.points_per_second {
        println!("points_per_second: {rate:.1}");
    }
    if let Some(rate) = report.docs_per_second {
        println!("docs_per_second: {rate:.1}");
    }
    if let Some(rate) = report.chunks_per_second {
        println!("chunks_per_second: {rate:.1}");
    }
    if let Some(rate) = report.files_per_second {
        println!("files_per_second: {rate:.1}");
    }
    print_tei_metrics(report);
    print_vllm_metrics(report);
    print_qdrant_metrics(report);
    println!("cleaned_up: {}", report.cleaned_up);
}

fn print_tei_metrics(report: &BenchReport) {
    if let Some(delta) = report.tei_embed_count_delta {
        println!("tei_embed_count_delta: {delta:.0}");
    }
    if let Some(delta) = report.tei_request_success_delta {
        println!("tei_request_success_delta: {delta:.0}");
    }
    if let Some(avg) = report.tei_avg_inputs_per_request {
        println!("tei_avg_inputs_per_request: {avg:.1}");
    }
    if let Some(delta) = report.tei_input_tokens_delta {
        println!("tei_input_tokens_delta: {delta:.0}");
    }
    if let Some(delta) = report.tei_input_count_delta {
        println!("tei_input_count_delta: {delta:.0}");
    }
    if let Some(avg) = report.tei_avg_input_tokens {
        println!("tei_avg_input_tokens: {avg:.1}");
    }
    if let Some(rate) = report.tei_input_tokens_per_second {
        println!("tei_input_tokens_per_second: {rate:.1}");
    }
    if let Some(delta) = report.tei_embed_duration_seconds_delta {
        println!("tei_embed_duration_seconds_delta: {delta:.3}");
    }
    if let Some(delta) = report.tei_inference_duration_seconds_delta {
        println!("tei_inference_duration_seconds_delta: {delta:.3}");
    }
    if let Some(delta) = report.tei_queue_duration_seconds_delta {
        println!("tei_queue_duration_seconds_delta: {delta:.3}");
    }
    if let Some(delta) = report.tei_tokenization_duration_seconds_delta {
        println!("tei_tokenization_duration_seconds_delta: {delta:.3}");
    }
    if let Some(avg) = report.tei_avg_embed_seconds_per_input {
        println!("tei_avg_embed_seconds_per_input: {avg:.6}");
    }
    if let Some(avg) = report.tei_avg_inference_seconds_per_input {
        println!("tei_avg_inference_seconds_per_input: {avg:.6}");
    }
    if let Some(delta) = report.tei_request_failure_overloaded_delta {
        println!("tei_overloaded_delta: {delta:.0}");
    }
}

fn print_vllm_metrics(report: &BenchReport) {
    if let Some(delta) = report.vllm_request_success_delta {
        println!("vllm_request_success_delta: {delta:.0}");
    }
    if let Some(delta) = report.vllm_prompt_tokens_delta {
        println!("vllm_prompt_tokens_delta: {delta:.0}");
    }
    if let Some(delta) = report.vllm_request_prompt_tokens_count_delta {
        println!("vllm_request_prompt_tokens_count_delta: {delta:.0}");
    }
    if let Some(avg) = report.vllm_avg_prompt_tokens {
        println!("vllm_avg_prompt_tokens: {avg:.1}");
    }
    if let Some(rate) = report.vllm_prompt_tokens_per_second {
        println!("vllm_prompt_tokens_per_second: {rate:.1}");
    }
    if let Some(delta) = report.vllm_time_to_first_token_seconds_delta {
        println!("vllm_time_to_first_token_seconds_delta: {delta:.3}");
    }
    if let Some(avg) = report.vllm_avg_time_to_first_token_seconds {
        println!("vllm_avg_time_to_first_token_seconds: {avg:.6}");
    }
}

fn print_qdrant_metrics(report: &BenchReport) {
    if let Some(delta) = report.qdrant_upsert_requests_delta {
        println!("qdrant_upsert_requests_delta: {delta:.0}");
    }
    if let Some(delta) = report.qdrant_upsert_duration_seconds_delta {
        println!("qdrant_upsert_duration_seconds_delta: {delta:.3}");
    }
    if let Some(avg) = report.qdrant_avg_upsert_seconds_per_request {
        println!("qdrant_avg_upsert_seconds_per_request: {avg:.6}");
    }
    if let Some(delta) = report.qdrant_index_requests_delta {
        println!("qdrant_index_requests_delta: {delta:.0}");
    }
    if let Some(delta) = report.qdrant_index_duration_seconds_delta {
        println!("qdrant_index_duration_seconds_delta: {delta:.3}");
    }
}

#[cfg(test)]
mod tests {
    use super::parse_prometheus_metrics;

    #[test]
    fn parses_prometheus_metric_lines() {
        let metrics = parse_prometheus_metrics(
            r#"
# TYPE te_request_failure counter
te_request_failure{err="overloaded"} 12
te_embed_count 42
te_request_input_length_sum 8192
te_request_input_length_count 16
rest_responses_total{method="PUT",endpoint="/collections/{name}/points",status="200"} 3
"#,
        );

        assert_eq!(
            metrics.get(r#"te_request_failure{err="overloaded"}"#),
            Some(&12.0)
        );
        assert_eq!(metrics.get("te_embed_count"), Some(&42.0));
        assert_eq!(metrics.get("te_request_input_length_sum"), Some(&8192.0));
        assert_eq!(metrics.get("te_request_input_length_count"), Some(&16.0));
        assert_eq!(
            metrics.get(
                r#"rest_responses_total{method="PUT",endpoint="/collections/{name}/points",status="200"}"#
            ),
            Some(&3.0)
        );
    }
}
