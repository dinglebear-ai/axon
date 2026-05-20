//! SQLite-runtime doctor report: SQLite + HTTP services only (no PG/Redis/AMQP probes).

use crate::cli::commands::probe::with_path;
use crate::cli::route::{CommandRoute, plan_command_route};
use crate::core::config::Config;
use crate::core::endpoints::{EndpointKind, resolve_host_endpoint};
use crate::core::health::browser_diagnostics_pattern;
use crate::core::health::doctor::{
    build_browser_runtime, probe_tei_info, tei_info_summary, tei_model_from_info, timed_probe,
};
use crate::core::http::internal_service_http_client;
use serde_json::{Map, Value};
use std::error::Error;
use std::time::Duration;

/// SQLite-runtime doctor: skip PG/Redis/AMQP probes, check SQLite file and HTTP services.
pub(super) async fn build(cfg: &Config) -> Result<Value, Box<dyn Error>> {
    let diagnostics = browser_diagnostics_pattern();
    let probes = collect_service_probes(cfg).await;

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
    services.insert(
        "gemini_headless".to_string(),
        gemini_service_json(cfg, &gemini_probe),
    );

    let effective_qdrant = resolve_host_endpoint(EndpointKind::Qdrant, Some(&cfg.qdrant_url), &[]);
    let effective_tei = resolve_host_endpoint(EndpointKind::Embedding, Some(&cfg.tei_url), &[]);
    let route = plan_command_route(cfg, &cfg.positional)
        .map(|plan| match plan.route {
            CommandRoute::PreferServer => "server",
            CommandRoute::LocalOnly => "local",
        })
        .unwrap_or("local");

    Ok(serde_json::json!({
        "observed_at_utc": chrono::Utc::now().to_rfc3339(),
        "mode": {
            "client": cfg.client_mode.to_string(),
            "server_url": cfg.server_url.as_ref().map(reqwest::Url::to_string),
            "route": route,
            "fallback": false,
            "local_runtime": "sqlite_in_process",
        },
        "capabilities": [
            {
                "tier": "tier_1_crawl_retrieve",
                "available": qdrant_ok,
                "impact": ["crawl, retrieve, and query require Qdrant for indexed data"],
                "remedies": if qdrant_ok { Vec::<String>::new() } else { vec!["start qdrant with `just services-up`".to_string()] },
            },
            {
                "tier": "tier_2_embedding",
                "available": tei_ok,
                "impact": ["embed and semantic search require TEI embeddings"],
                "remedies": if tei_ok { Vec::<String>::new() } else { vec!["start TEI or configure TEI_URL".to_string()] },
            }
        ],
        "recommendations": [
            "Use AXON_SERVER_URL for REST server mode; add --local for explicit local execution."
        ],
        "effective_endpoints": {
            "qdrant": effective_qdrant,
            "embedding": effective_tei,
        },
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
    client_ok: bool,
}

async fn collect_service_probes(cfg: &Config) -> ServiceProbes {
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
            let (tei_info, _) = timed_probe(probe_tei_info(&cfg.tei_url, client)).await;

            ServiceProbes {
                tei,
                tei_latency_ms,
                tei_info,
                qdrant,
                chrome,
                client_ok: true,
            }
        }
        Err(_) => failed_service_probes(client_err_detail),
    }
}

fn failed_service_probes(detail: Option<String>) -> ServiceProbes {
    let failed = (false, detail.clone());
    let tei_info = (None, detail);

    ServiceProbes {
        tei: failed.clone(),
        tei_latency_ms: 0,
        tei_info,
        qdrant: failed.clone(),
        chrome: failed,
        client_ok: false,
    }
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
        "configured_url": cfg.tei_url,
        "effective_url": resolve_host_endpoint(
            EndpointKind::Embedding,
            Some(&cfg.tei_url),
            &[],
        ).map(|endpoint| endpoint.url),
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
        "configured_url": cfg.qdrant_url,
        "effective_url": resolve_host_endpoint(
            EndpointKind::Qdrant,
            Some(&cfg.qdrant_url),
            &[],
        ).map(|endpoint| endpoint.url),
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
