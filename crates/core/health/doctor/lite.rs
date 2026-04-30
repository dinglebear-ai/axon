//! Lite-mode doctor report: SQLite + HTTP services only (no PG/Redis/AMQP probes).

use crate::crates::cli::commands::probe::probe_http;
use crate::crates::core::config::Config;
use crate::crates::core::health::browser_diagnostics_pattern;
use crate::crates::core::health::doctor::{
    build_browser_runtime, probe_chrome, probe_openai, probe_tei_info, resolve_openai_model,
    tei_info_summary, tei_model_from_info, timed_probe,
};
use crate::crates::core::http::build_client;
use serde_json::Value;
use std::error::Error;
use std::time::Duration;

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
        timed_probe(probe_chrome(cfg.chrome_remote_url.as_deref())),
    );

    let (tei_info_probe, openai_probe, openai_probe_ms) = match probe_client_result {
        Ok(ref client) => {
            let ((tei_info, _), (openai, openai_ms)) = spider::tokio::join!(
                timed_probe(probe_tei_info(&cfg.tei_url, client)),
                timed_probe(probe_openai(cfg, &openai_model, client)),
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
    let browser_runtime = build_browser_runtime(&diagnostics);

    // Vector mode mismatch probe: when Qdrant is reachable, look at the active
    // collection's vectors block and compare to cfg.hybrid_search_enabled. If
    // the collection is unnamed (legacy) but hybrid_search_enabled is on,
    // hybrid search will silently fall back to dense-only — operators need to
    // see this. (bd axon_rust-d71.4)
    let vector_mode = if qdrant_ok && probe_client_result.is_ok() {
        probe_vector_mode(&cfg.qdrant_url, &cfg.collection).await
    } else {
        None
    };
    let vector_mode_str = vector_mode.as_deref();
    let vector_mode_mismatch = match vector_mode_str {
        Some("unnamed") if cfg.hybrid_search_enabled => Some(
            "collection is in legacy unnamed-vector mode but hybrid_search_enabled=true; \
             hybrid RRF search will fall back to dense-only — run `axon migrate` to upgrade",
        ),
        _ => None,
    };

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
                "collection": cfg.collection,
                "vector_mode": vector_mode_str,
                "hybrid_search_enabled": cfg.hybrid_search_enabled,
                "mode_mismatch_warning": vector_mode_mismatch,
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
        "all_ok": tei_ok && qdrant_ok && vector_mode_mismatch.is_none(),
    }))
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
