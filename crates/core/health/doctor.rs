pub(crate) mod lite;

use crate::crates::cli::commands::probe::{probe_http, with_path};
use crate::crates::core::config::Config;
use crate::crates::core::content::redact_url;
use crate::crates::core::health::browser_diagnostics_pattern;
use crate::crates::core::http::build_client;
use crate::crates::core::logging::log_info;
use crate::crates::jobs::common::count_stale_and_pending_jobs;
use crate::crates::jobs::crawl::doctor as crawl_doctor;
use crate::crates::jobs::embed::embed_doctor;
use crate::crates::jobs::extract::extract_doctor;
use crate::crates::jobs::ingest::ingest_doctor;
use serde_json::Value;
use std::env;
use std::error::Error;
use std::future::Future;
use std::time::Instant;

pub(super) fn elapsed_ms(start: Instant) -> u64 {
    let ms = start.elapsed().as_millis();
    if ms > u128::from(u64::MAX) {
        u64::MAX
    } else {
        ms as u64
    }
}

pub(super) async fn probe_tei_info(
    url: &str,
    client: &reqwest::Client,
) -> (Option<Value>, Option<String>) {
    if url.trim().is_empty() {
        return (None, Some("not configured".to_string()));
    }

    let mut last_error = None;
    for path in ["/info", "/v1/info"] {
        let endpoint = with_path(url, path);
        match client.get(endpoint).send().await {
            Ok(resp) if resp.status().is_success() => {
                let status = resp.status();
                match resp.json::<Value>().await {
                    Ok(json) => return (Some(json), Some(format!("{path} {status}"))),
                    Err(err) => last_error = Some(format!("{path} invalid json: {err}")),
                }
            }
            Ok(resp) => last_error = Some(format!("{path} {}", resp.status())),
            Err(err) => last_error = Some(err.to_string()),
        }
    }

    (None, last_error)
}

pub(super) fn tei_model_from_info(info: &Value) -> Option<String> {
    let str_field = |key: &str| info.get(key).and_then(Value::as_str).map(str::to_string);
    str_field("model_id")
        .or_else(|| str_field("model_name"))
        .or_else(|| {
            let model = info.get("model")?;
            model
                .as_str()
                .or_else(|| model.get("id").and_then(Value::as_str))
                .or_else(|| model.get("name").and_then(Value::as_str))
                .map(str::to_string)
        })
}

pub(super) fn tei_info_summary(info: &Value) -> Option<String> {
    let mut parts = Vec::new();
    for key in [
        "model_sha",
        "max_concurrent_requests",
        "max_client_batch_size",
        "max_batch_total_tokens",
        "max_input_tokens",
        "max_input_length",
    ] {
        if let Some(value) = info.get(key) {
            parts.push(format!("{key}={value}"));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

pub(super) fn resolve_openai_model(cfg: &Config) -> String {
    if !cfg.openai_model.trim().is_empty() {
        return cfg.openai_model.clone();
    }

    env::var("OPENAI_MODEL").unwrap_or_default()
}

/// Live probe: GET `{base}/models` with a 3s timeout. Returns (ok, detail).
///
/// **Important:** `openai_base_url` must include the `/v1` path component
/// (e.g. `http://host:8080/v1`). If it's missing, the probe will hit
/// `http://host:8080/models` instead of `http://host:8080/v1/models`
/// and likely return a 404.
pub(super) async fn probe_openai(
    cfg: &Config,
    openai_model: &str,
    client: &reqwest::Client,
) -> (bool, String) {
    let base = cfg.openai_base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return (false, "not configured".to_string());
    }
    if openai_model.trim().is_empty() {
        return (false, "OPENAI_MODEL not set".to_string());
    }

    let url = format!("{base}/models");
    let mut req = client.get(&url);
    if !cfg.openai_api_key.trim().is_empty() {
        req = req.bearer_auth(&cfg.openai_api_key);
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            (true, format!("http {} /models", resp.status().as_u16()))
        }
        Ok(resp) => (false, format!("http {} /models", resp.status().as_u16())),
        Err(e) => (false, e.to_string()),
    }
}

/// Probe the Chrome CDP management endpoint. Returns (ok, detail).
pub(super) async fn probe_chrome(chrome_url: Option<&str>) -> (bool, Option<String>) {
    let url = match chrome_url {
        Some(u) if !u.trim().is_empty() => u,
        _ => return (false, None),
    };

    // CDP /json/version is the canonical health endpoint.
    probe_http(url, &["/json/version", "/json"]).await
}

struct DoctorProbes {
    crawl_report: Value,
    crawl_report_ms: u64,
    extract_report: Value,
    extract_report_ms: u64,
    embed_report: Value,
    embed_report_ms: u64,
    ingest_report: Value,
    ingest_report_ms: u64,
    tei_probe: (bool, Option<String>),
    tei_probe_ms: u64,
    tei_info_probe: (Option<Value>, Option<String>),
    tei_info_probe_ms: u64,
    qdrant_probe: (bool, Option<String>),
    qdrant_probe_ms: u64,
    chrome_probe: (bool, Option<String>),
    chrome_probe_ms: u64,
    openai_probe: (bool, String),
    openai_probe_ms: u64,
    stale_jobs: Option<(i64, i64)>,
    stale_jobs_ms: u64,
}

pub(super) async fn timed_probe<T, F>(future: F) -> (T, u64)
where
    F: Future<Output = T>,
{
    let start = Instant::now();
    let value = future.await;
    (value, elapsed_ms(start))
}

async fn gather_doctor_probes(
    cfg: &Config,
    openai_model: &str,
) -> Result<DoctorProbes, Box<dyn Error>> {
    // Single short-timeout client shared across all health probes — avoids
    // constructing multiple reqwest Clients (TLS backend init + pool setup each).
    // If client init fails, surface the error as per-probe failures for TEI and
    // OpenAI rather than aborting the entire doctor report.
    let probe_client_result = build_client(5, None);
    let client_err_detail = probe_client_result
        .as_ref()
        .err()
        .map(|e| format!("http client init failed: {e}"));

    // Run all probes that don't require the HTTP client in parallel unconditionally.
    // Probes that need the client are skipped (with failure detail) if init failed.
    let (
        (crawl_report, crawl_report_ms),
        (extract_report, extract_report_ms),
        (embed_report, embed_report_ms),
        (ingest_report, ingest_report_ms),
        (tei_probe, tei_probe_ms),
        (qdrant_probe, qdrant_probe_ms),
        (chrome_probe, chrome_probe_ms),
        (stale_jobs, stale_jobs_ms),
    ) = spider::tokio::join!(
        timed_probe(crawl_doctor(cfg)),
        timed_probe(extract_doctor(cfg)),
        timed_probe(embed_doctor(cfg)),
        timed_probe(ingest_doctor(cfg)),
        timed_probe(probe_http(&cfg.tei_url, &["/health", "/"])),
        timed_probe(probe_http(&cfg.qdrant_url, &["/healthz", "/"])),
        timed_probe(probe_chrome(cfg.chrome_remote_url.as_deref())),
        timed_probe(count_stale_and_pending_jobs(cfg, 15)),
    );

    // Probes requiring the HTTP client: run only when client init succeeded, otherwise
    // synthesize a failure result so the rest of the report is unaffected.
    let (tei_info_probe, tei_info_probe_ms, openai_probe, openai_probe_ms) =
        match probe_client_result {
            Ok(ref client) => {
                let ((tei_info, tei_info_ms), (openai, openai_ms)) = spider::tokio::join!(
                    timed_probe(probe_tei_info(&cfg.tei_url, client)),
                    timed_probe(probe_openai(cfg, openai_model, client)),
                );
                (tei_info, tei_info_ms, openai, openai_ms)
            }
            Err(_) => {
                let detail = client_err_detail.clone();
                let tei_fail: (Option<Value>, Option<String>) = (None, detail.clone());
                let openai_fail: (bool, String) = (
                    false,
                    detail.unwrap_or_else(|| "http client init failed".to_string()),
                );
                (tei_fail, 0u64, openai_fail, 0u64)
            }
        };

    Ok(DoctorProbes {
        crawl_report: crawl_report?,
        crawl_report_ms,
        extract_report: extract_report?,
        extract_report_ms,
        embed_report: embed_report?,
        embed_report_ms,
        ingest_report: ingest_report?,
        ingest_report_ms,
        tei_probe,
        tei_probe_ms,
        tei_info_probe,
        tei_info_probe_ms,
        qdrant_probe,
        qdrant_probe_ms,
        chrome_probe,
        chrome_probe_ms,
        openai_probe,
        openai_probe_ms,
        stale_jobs,
        stale_jobs_ms,
    })
}

fn build_pipeline_status(probes: &DoctorProbes, openai_ok: bool) -> Value {
    // extract is degraded (not failed) when OpenAI is unconfigured — jobs will be
    // queued but will fail at the LLM step. Surface that here so the user sees it.
    let extract_infra_ok = probes.extract_report["all_ok"].as_bool().unwrap_or(false);
    serde_json::json!({
        "crawl": probes.crawl_report["all_ok"].as_bool().unwrap_or(false),
        "extract": extract_infra_ok,
        "extract_llm_ready": extract_infra_ok && openai_ok,
        "embed": probes.embed_report["all_ok"].as_bool().unwrap_or(false),
        "ingest": probes.ingest_report["all_ok"].as_bool().unwrap_or(false),
    })
}

fn build_browser_runtime(
    diagnostics: &crate::crates::core::health::BrowserDiagnosticsPattern,
) -> Value {
    serde_json::json!({
        "selection": "chrome",
        "diagnostics": {
            "enabled": diagnostics.enabled,
            "screenshot": diagnostics.screenshot,
            "events": diagnostics.events,
            "output_dir": diagnostics.output_dir,
        },
    })
}

fn build_services_status(cfg: &Config, probes: &DoctorProbes, openai_model: &str) -> Value {
    let tei_info = probes.tei_info_probe.0.clone();
    let tei_info_detail = probes.tei_info_probe.1.clone();
    let tei_model = tei_info.as_ref().and_then(tei_model_from_info);
    let tei_summary = tei_info.as_ref().and_then(tei_info_summary);
    let (openai_live_ok, ref openai_live_detail) = probes.openai_probe;
    let (chrome_ok, ref chrome_detail) = probes.chrome_probe;

    serde_json::json!({
        "postgres": {
            "ok": probes.crawl_report["postgres_ok"].as_bool().unwrap_or(false),
            "url": redact_url(&cfg.pg_url),
        },
        "redis": {
            "ok": probes.crawl_report["redis_ok"].as_bool().unwrap_or(false),
            "url": redact_url(&cfg.redis_url),
        },
        "amqp": {
            "ok": probes.crawl_report["amqp_ok"].as_bool().unwrap_or(false),
            "url": redact_url(&cfg.amqp_url),
        },
        "tei": {
            "ok": probes.tei_probe.0,
            "url": cfg.tei_url,
            "detail": probes.tei_probe.1.clone(),
            "info_detail": tei_info_detail,
            "model": tei_model,
            "summary": tei_summary,
            "info": tei_info,
            "latency_ms": probes.tei_probe_ms,
            "info_latency_ms": probes.tei_info_probe_ms,
        },
        "qdrant": {
            "ok": probes.qdrant_probe.0,
            "url": cfg.qdrant_url,
            "detail": probes.qdrant_probe.1.clone(),
            "latency_ms": probes.qdrant_probe_ms,
        },
        "chrome": {
            "ok": chrome_ok,
            "configured": cfg.chrome_remote_url.is_some(),
            "url": cfg.chrome_remote_url,
            "detail": chrome_detail,
            "latency_ms": probes.chrome_probe_ms,
        },
        "openai": {
            "ok": openai_live_ok,
            "configured": !cfg.openai_base_url.trim().is_empty() && !openai_model.trim().is_empty(),
            "detail": openai_live_detail,
            "base_url": cfg.openai_base_url,
            "model": openai_model,
            "latency_ms": probes.openai_probe_ms,
        },
    })
}

fn build_queue_names(cfg: &Config) -> Value {
    serde_json::json!({
        "crawl": cfg.crawl_queue,
        "extract": cfg.extract_queue,
        "embed": cfg.embed_queue,
        "ingest": cfg.ingest_queue,
    })
}

fn report_overall_ok(pipelines: &Value, tei_ok: bool, qdrant_ok: bool) -> bool {
    pipelines["crawl"].as_bool().unwrap_or(false)
        && pipelines["extract"].as_bool().unwrap_or(false)
        && pipelines["embed"].as_bool().unwrap_or(false)
        && pipelines["ingest"].as_bool().unwrap_or(false)
        && tei_ok
        && qdrant_ok
}

// NOTE: run_doctor delegates to build_doctor_report and only renders output.
// Keep probe logic centralized in build_doctor_report to avoid drift.
pub async fn build_doctor_report(cfg: &Config) -> Result<Value, Box<dyn Error>> {
    if cfg.lite_mode {
        return lite::build(cfg).await;
    }
    let diagnostics = browser_diagnostics_pattern();
    let openai_model = resolve_openai_model(cfg);
    let probes = gather_doctor_probes(cfg, &openai_model).await?;
    let (openai_live_ok, _) = probes.openai_probe.clone();

    // Structured per-service probe logging
    for (service, ok) in [
        ("tei", probes.tei_probe.0),
        ("qdrant", probes.qdrant_probe.0),
        ("chrome", probes.chrome_probe.0),
        ("openai", openai_live_ok),
        (
            "crawl_infra",
            probes.crawl_report["all_ok"].as_bool().unwrap_or(false),
        ),
    ] {
        log_info(&format!(
            "doctor probe service={service} status={}",
            if ok { "ok" } else { "fail" }
        ));
    }

    let pipelines = build_pipeline_status(&probes, openai_live_ok);
    let services = build_services_status(cfg, &probes, &openai_model);
    let queue_names = build_queue_names(cfg);
    let browser_runtime = build_browser_runtime(&diagnostics);
    let all_ok = report_overall_ok(&pipelines, probes.tei_probe.0, probes.qdrant_probe.0);

    let (stale_count, pending_count) = probes.stale_jobs.unwrap_or((0, 0));

    Ok(serde_json::json!({
        "observed_at_utc": chrono::Utc::now().to_rfc3339(),
        "services": services,
        "pipelines": pipelines,
        "queue_names": queue_names,
        "browser_runtime": browser_runtime,
        "stale_jobs": stale_count,
        "pending_jobs": pending_count,
        "timing_ms": {
            "crawl_report": probes.crawl_report_ms,
            "extract_report": probes.extract_report_ms,
            "embed_report": probes.embed_report_ms,
            "ingest_report": probes.ingest_report_ms,
            "stale_pending": probes.stale_jobs_ms
        },
        "all_ok": all_ok
    }))
}
