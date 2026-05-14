//! Lite-mode doctor report: SQLite + HTTP services only (no PG/Redis/AMQP probes).

use crate::cli::commands::probe::with_path;
use crate::core::config::Config;
use crate::core::health::browser_diagnostics_pattern;
use crate::core::health::doctor::{
    build_browser_runtime, probe_openai, probe_tei_info, resolve_openai_model, tei_info_summary,
    tei_model_from_info, timed_probe,
};
use crate::core::http::internal_service_http_client;
use serde_json::{Map, Value};
use std::error::Error;
use std::time::Duration;

/// Lite-mode doctor: skip PG/Redis/AMQP probes, check SQLite file and HTTP services.
pub(super) async fn build(cfg: &Config) -> Result<Value, Box<dyn Error>> {
    let diagnostics = browser_diagnostics_pattern();
    let openai_model = resolve_openai_model(cfg);
    let openai_enabled = openai_diagnostics_enabled(cfg, &openai_model);
    let probes = collect_service_probes(cfg, &openai_model, openai_enabled).await;

    let sqlite_path = cfg.sqlite_path.display().to_string();
    let sqlite_exists = cfg.sqlite_path.exists();
    let gemini_probe = probe_gemini_headless(cfg);
    let tei_model = probes.tei_info.0.as_ref().and_then(tei_model_from_info);
    let tei_summary = probes.tei_info.0.as_ref().and_then(tei_info_summary);
    let (chrome_ok, ref chrome_detail) = probes.chrome;
    let tei_ok = probes.tei.0;
    let qdrant_ok = probes.qdrant.0;
    let browser_runtime = build_browser_runtime(&diagnostics);

    let vector_mode = probe_vector_mode_if_reachable(cfg, qdrant_ok, probes.client_ok).await;
    let vector_mode_str = vector_mode.as_deref();
    let vector_mode_mismatch = vector_mode_mismatch_warning(vector_mode_str, cfg);
    let mut services = Map::new();
    services.insert(
        "sqlite".to_string(),
        sqlite_service_json(sqlite_exists, &sqlite_path),
    );
    services.insert(
        "tei".to_string(),
        tei_service_json(
            cfg,
            tei_ok,
            probes.tei.1,
            tei_model,
            tei_summary,
            probes.tei_latency_ms,
        ),
    );
    services.insert(
        "qdrant".to_string(),
        qdrant_service_json(
            cfg,
            qdrant_ok,
            probes.qdrant.1,
            vector_mode_str,
            vector_mode_mismatch,
        ),
    );
    services.insert(
        "chrome".to_string(),
        chrome_service_json(cfg, chrome_ok, chrome_detail),
    );
    if let Some(openai) = probes.openai_service {
        services.insert("openai".to_string(), openai);
    }
    services.insert(
        "gemini_headless".to_string(),
        gemini_service_json(cfg, &gemini_probe),
    );

    Ok(serde_json::json!({
        "observed_at_utc": chrono::Utc::now().to_rfc3339(),
        "lite_mode": true,
        "services": Value::Object(services),
        "pipelines": {
            "crawl": true,
            "extract": true,
            "extract_llm_ready": gemini_probe.0,
            "embed": true,
            "ingest": true,
        },
        "queue_names": {},
        "browser_runtime": browser_runtime,
        "stale_jobs": 0_i64,
        "pending_jobs": 0_i64,
        "all_ok": tei_ok && qdrant_ok && vector_mode_mismatch.is_none(),
    }))
}

struct ServiceProbes {
    tei: (bool, Option<String>),
    tei_latency_ms: u64,
    tei_info: (Option<Value>, Option<String>),
    qdrant: (bool, Option<String>),
    chrome: (bool, Option<String>),
    openai_service: Option<Value>,
    client_ok: bool,
}

async fn collect_service_probes(
    cfg: &Config,
    openai_model: &str,
    openai_enabled: bool,
) -> ServiceProbes {
    let probe_client_result = internal_service_http_client();
    let client_err_detail = probe_client_result
        .as_ref()
        .err()
        .map(|e| format!("http client init failed: {e}"));

    match probe_client_result {
        Ok(client) => {
            let chrome_url = cfg.chrome_remote_url.as_deref();
            let ((tei, tei_latency_ms), (qdrant, _), (chrome, _)) = spider::tokio::join!(
                timed_probe(probe_internal_http(client, &cfg.tei_url, &["/health", "/"])),
                timed_probe(probe_internal_http(
                    client,
                    &cfg.qdrant_url,
                    &["/healthz", "/"]
                )),
                timed_probe(probe_internal_chrome(client, chrome_url)),
            );
            let (tei_info, openai_service) =
                collect_model_probes(cfg, client, openai_model, openai_enabled).await;

            ServiceProbes {
                tei,
                tei_latency_ms,
                tei_info,
                qdrant,
                chrome,
                openai_service,
                client_ok: true,
            }
        }
        Err(_) => failed_service_probes(cfg, openai_model, openai_enabled, client_err_detail),
    }
}

async fn collect_model_probes(
    cfg: &Config,
    client: &reqwest::Client,
    openai_model: &str,
    openai_enabled: bool,
) -> ((Option<Value>, Option<String>), Option<Value>) {
    if openai_enabled {
        let ((tei_info, _), (openai, openai_ms)) = spider::tokio::join!(
            timed_probe(probe_tei_info(&cfg.tei_url, client)),
            timed_probe(probe_openai(cfg, openai_model, client)),
        );
        (
            tei_info,
            Some(openai_service_json(cfg, openai_model, openai, openai_ms)),
        )
    } else {
        let (tei_info, _) = timed_probe(probe_tei_info(&cfg.tei_url, client)).await;
        (tei_info, None)
    }
}

fn failed_service_probes(
    cfg: &Config,
    openai_model: &str,
    openai_enabled: bool,
    detail: Option<String>,
) -> ServiceProbes {
    let failed = (false, detail.clone());
    let tei_info = (None, detail.clone());
    let openai_service = openai_enabled.then(|| {
        let openai_fail = (
            false,
            detail.unwrap_or_else(|| "http client init failed".to_string()),
        );
        openai_service_json(cfg, openai_model, openai_fail, 0)
    });

    ServiceProbes {
        tei: failed.clone(),
        tei_latency_ms: 0,
        tei_info,
        qdrant: failed.clone(),
        chrome: failed,
        openai_service,
        client_ok: false,
    }
}

fn openai_diagnostics_enabled(cfg: &Config, openai_model: &str) -> bool {
    !cfg.openai_base_url.trim().is_empty() && !openai_model.trim().is_empty()
}

async fn probe_internal_chrome(
    client: &reqwest::Client,
    chrome_url: Option<&str>,
) -> (bool, Option<String>) {
    match chrome_url {
        Some(url) if !url.trim().is_empty() => {
            probe_internal_http(client, url, &["/json/version", "/json"]).await
        }
        _ => (false, None),
    }
}

async fn probe_internal_http(
    client: &reqwest::Client,
    url: &str,
    paths: &[&str],
) -> (bool, Option<String>) {
    if url.trim().is_empty() {
        return (false, Some("not configured".to_string()));
    }

    let mut last_error = None;
    for path in paths {
        let endpoint = with_path(url, path);
        match client.get(endpoint).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() || status.is_redirection() {
                    return (true, Some(format!("http {}", status.as_u16())));
                }
                last_error = Some(format!("http {}", status.as_u16()));
            }
            Err(err) => last_error = Some(err.to_string()),
        }
    }

    (false, last_error)
}

fn sqlite_service_json(exists: bool, path: &str) -> Value {
    serde_json::json!({
        "ok": true,
        "exists": exists,
        "path": path,
    })
}

fn tei_service_json(
    cfg: &Config,
    ok: bool,
    detail: Option<String>,
    model: Option<String>,
    summary: Option<String>,
    latency_ms: u64,
) -> Value {
    serde_json::json!({
        "ok": ok,
        "url": cfg.tei_url,
        "detail": detail,
        "model": model,
        "summary": summary,
        "latency_ms": latency_ms,
    })
}

fn qdrant_service_json(
    cfg: &Config,
    ok: bool,
    detail: Option<String>,
    vector_mode: Option<&str>,
    mode_mismatch: Option<&str>,
) -> Value {
    serde_json::json!({
        "ok": ok,
        "url": cfg.qdrant_url,
        "detail": detail,
        "collection": cfg.collection,
        "vector_mode": vector_mode,
        "hybrid_search_enabled": cfg.hybrid_search_enabled,
        "mode_mismatch_warning": mode_mismatch,
    })
}

fn chrome_service_json(cfg: &Config, ok: bool, detail: &Option<String>) -> Value {
    serde_json::json!({
        "ok": ok,
        "configured": cfg.chrome_remote_url.is_some(),
        "url": cfg.chrome_remote_url,
        "detail": detail,
    })
}

fn gemini_service_json(cfg: &Config, probe: &(bool, String)) -> Value {
    let model = if cfg.headless_gemini_model.trim().is_empty() {
        Value::Null
    } else {
        serde_json::json!(&cfg.headless_gemini_model)
    };
    serde_json::json!({
        "ok": probe.0,
        "configured": true,
        "detail": probe.1,
        "command": cfg.headless_gemini_cmd,
        "model": model,
    })
}

fn openai_service_json(
    cfg: &Config,
    openai_model: &str,
    openai_probe: (bool, String),
    latency_ms: u64,
) -> Value {
    let (openai_live_ok, openai_live_detail) = openai_probe;
    serde_json::json!({
        "ok": openai_live_ok,
        "configured": !cfg.openai_base_url.trim().is_empty() && !openai_model.trim().is_empty(),
        "detail": openai_live_detail,
        "base_url": cfg.openai_base_url,
        "model": openai_model,
        "latency_ms": latency_ms,
    })
}

fn probe_gemini_headless(cfg: &Config) -> (bool, String) {
    let gemini_backend = crate::services::llm_backend::LlmBackendConfig::from_config(cfg);
    match crate::services::llm_backend::headless::gemini::validate_config(&gemini_backend) {
        Ok(()) => (
            true,
            "Gemini headless command validation passed".to_string(),
        ),
        Err(err) => (false, err.to_string()),
    }
}

async fn probe_vector_mode_if_reachable(
    cfg: &Config,
    qdrant_ok: bool,
    client_ok: bool,
) -> Option<String> {
    if qdrant_ok && client_ok {
        probe_vector_mode(&cfg.qdrant_url, &cfg.collection).await
    } else {
        None
    }
}

fn vector_mode_mismatch_warning(vector_mode: Option<&str>, cfg: &Config) -> Option<&'static str> {
    match vector_mode {
        Some("unnamed") if cfg.hybrid_search_enabled => Some(
            "collection is in legacy unnamed-vector mode but hybrid_search_enabled=true; \
             hybrid RRF search will fall back to dense-only — run `axon migrate` to upgrade",
        ),
        _ => None,
    }
}

/// GET `/collections/{name}` and classify the vectors block as named/unnamed.
///
/// Returns `Some("named")` when `result.config.params.vectors` contains a
/// `dense` (or `bm42`) entry, `Some("unnamed")` when it has a bare `size`
/// field, and `None` if the collection is missing or the response shape is
/// unexpected. Best-effort — never fails the doctor probe.
async fn probe_vector_mode(qdrant_url: &str, collection: &str) -> Option<String> {
    let url = format!(
        "{}/collections/{}",
        qdrant_url.trim_end_matches('/'),
        collection
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: Value = resp.json().await.ok()?;
    let vectors = body
        .get("result")?
        .get("config")?
        .get("params")?
        .get("vectors")?;
    if vectors.get("size").is_some() {
        Some("unnamed".to_string())
    } else if vectors.is_object() {
        Some("named".to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::openai_diagnostics_enabled;
    use crate::core::config::Config;

    #[test]
    fn openai_diagnostics_are_disabled_without_openai_base_url() {
        let cfg = Config {
            headless_gemini_cmd: "gemini".to_string(),
            headless_gemini_model: "gemini-3.1-pro-preview".to_string(),
            ..Default::default()
        };

        assert!(!openai_diagnostics_enabled(&cfg, &cfg.openai_model));
    }

    #[test]
    fn openai_diagnostics_are_disabled_for_partial_openai_config() {
        let cfg = Config {
            openai_base_url: "http://localhost:11434/v1".to_string(),
            ..Default::default()
        };

        assert!(!openai_diagnostics_enabled(&cfg, &cfg.openai_model));
    }

    #[test]
    fn openai_diagnostics_are_reported_for_openai_compatible_base_url() {
        let cfg = Config {
            openai_base_url: "http://localhost:11434/v1".to_string(),
            openai_model: "llama3.2".to_string(),
            ..Default::default()
        };

        assert!(openai_diagnostics_enabled(&cfg, &cfg.openai_model));
    }
}
