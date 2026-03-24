//! Lite-mode doctor report: SQLite + HTTP services only (no PG/Redis/AMQP probes).

use crate::crates::cli::commands::probe::{probe_http, with_path};
use crate::crates::core::config::Config;
use crate::crates::core::health::browser_diagnostics_pattern;
use crate::crates::core::http::build_client;
use serde_json::Value;
use std::error::Error;
use std::future::Future;
use std::time::Instant;

fn elapsed_ms(start: Instant) -> u64 {
    let ms = start.elapsed().as_millis();
    if ms > u128::from(u64::MAX) {
        u64::MAX
    } else {
        ms as u64
    }
}

async fn timed_probe<T, F: Future<Output = T>>(future: F) -> (T, u64) {
    let start = Instant::now();
    let value = future.await;
    (value, elapsed_ms(start))
}

async fn probe_tei_info(url: &str, client: &reqwest::Client) -> (Option<Value>, Option<String>) {
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

fn tei_model_from_info(info: &Value) -> Option<String> {
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

fn tei_info_summary(info: &Value) -> Option<String> {
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

fn resolve_openai_model(cfg: &Config) -> String {
    if !cfg.openai_model.trim().is_empty() {
        return cfg.openai_model.clone();
    }
    std::env::var("OPENAI_MODEL").unwrap_or_default()
}

async fn probe_chrome_lite(chrome_url: Option<&str>) -> (bool, Option<String>) {
    let url = match chrome_url {
        Some(u) if !u.trim().is_empty() => u,
        _ => return (false, None),
    };
    probe_http(url, &["/json/version", "/json"]).await
}

async fn probe_openai_lite(
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

fn build_browser_runtime_lite(
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

/// Lite-mode doctor: skip PG/Redis/AMQP probes, check SQLite file and HTTP services.
pub(super) async fn build(cfg: &Config) -> Result<Value, Box<dyn Error>> {
    let diagnostics = browser_diagnostics_pattern();
    let openai_model = resolve_openai_model(cfg);
    let probe_client_result = build_client(5, None);
    let client_err_detail = probe_client_result
        .as_ref()
        .err()
        .map(|e| format!("http client init failed: {e}"));

    let (
        (tei_probe, tei_probe_ms),
        (qdrant_probe, _qdrant_probe_ms),
        (chrome_probe, _chrome_probe_ms),
    ) = spider::tokio::join!(
        timed_probe(probe_http(&cfg.tei_url, &["/health", "/"])),
        timed_probe(probe_http(&cfg.qdrant_url, &["/healthz", "/"])),
        timed_probe(probe_chrome_lite(cfg.chrome_remote_url.as_deref())),
    );

    let (tei_info_probe, openai_probe, openai_probe_ms) = match probe_client_result {
        Ok(ref client) => {
            let ((tei_info, _), (openai, openai_ms)) = spider::tokio::join!(
                timed_probe(probe_tei_info(&cfg.tei_url, client)),
                timed_probe(probe_openai_lite(cfg, &openai_model, client)),
            );
            (tei_info, openai, openai_ms)
        }
        Err(_) => {
            let detail = client_err_detail;
            let tei_fail: (Option<Value>, Option<String>) = (None, detail.clone());
            let openai_fail: (bool, String) = (
                false,
                detail.unwrap_or_else(|| "http client init failed".to_string()),
            );
            (tei_fail, openai_fail, 0u64)
        }
    };

    let sqlite_path = cfg.sqlite_path.display().to_string();
    let sqlite_exists = cfg.sqlite_path.exists();
    let tei_model = tei_info_probe.0.as_ref().and_then(tei_model_from_info);
    let tei_summary = tei_info_probe.0.as_ref().and_then(tei_info_summary);
    let (openai_live_ok, ref openai_live_detail) = openai_probe;
    let (chrome_ok, ref chrome_detail) = chrome_probe;
    let tei_ok = tei_probe.0;
    let qdrant_ok = qdrant_probe.0;
    let browser_runtime = build_browser_runtime_lite(&diagnostics);

    Ok(serde_json::json!({
        "observed_at_utc": chrono::Utc::now().to_rfc3339(),
        "lite_mode": true,
        "services": {
            "sqlite": {
                "ok": true,
                "exists": sqlite_exists,
                "path": sqlite_path,
            },
            "tei": {
                "ok": tei_ok,
                "url": cfg.tei_url,
                "detail": tei_probe.1,
                "model": tei_model,
                "summary": tei_summary,
                "latency_ms": tei_probe_ms,
            },
            "qdrant": {
                "ok": qdrant_ok,
                "url": cfg.qdrant_url,
                "detail": qdrant_probe.1,
            },
            "chrome": {
                "ok": chrome_ok,
                "configured": cfg.chrome_remote_url.is_some(),
                "url": cfg.chrome_remote_url,
                "detail": chrome_detail,
            },
            "openai": {
                "ok": openai_live_ok,
                "configured": !cfg.openai_base_url.trim().is_empty() && !openai_model.trim().is_empty(),
                "detail": openai_live_detail,
                "base_url": cfg.openai_base_url,
                "model": openai_model,
                "latency_ms": openai_probe_ms,
            },
        },
        "pipelines": {
            "crawl": true,
            "extract": true,
            "extract_llm_ready": openai_live_ok,
            "embed": true,
            "ingest": true,
        },
        "queue_names": {},
        "browser_runtime": browser_runtime,
        "stale_jobs": 0_i64,
        "pending_jobs": 0_i64,
        "all_ok": tei_ok && qdrant_ok,
    }))
}
