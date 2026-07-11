//! SQLite-runtime doctor report: SQLite + HTTP services only (no PG/Redis/AMQP probes).

use crate::config::Config;
use crate::endpoints::{EndpointKind, resolve_host_endpoint};
use crate::health::browser_diagnostics_pattern;
use crate::health::doctor::{
    LlmDoctorProbe, build_browser_runtime, probe_tei_info, tei_info_summary, tei_model_from_info,
    timed_probe,
};
use crate::http::internal_service_http_client;
use crate::http::with_path;
use crate::sqlite::diagnostics as sqlite_diagnostics;
use serde_json::{Map, Value};
use std::error::Error;
use std::time::Duration;

/// SQLite-runtime doctor: skip PG/Redis/AMQP probes, check SQLite file and HTTP services.
///
/// The LLM legs (round-trip, gemini validation, codex capabilities) are executed
/// by the caller through `axon-llm` and injected via [`LlmDoctorProbe`], because
/// the real backends live in `axon-llm` (which depends on `axon-core`).
pub(super) async fn build(
    cfg: &Config,
    pending_jobs: i64,
    llm_probe: LlmDoctorProbe,
) -> Result<Value, Box<dyn Error>> {
    let diagnostics = browser_diagnostics_pattern();
    let probes = collect_service_probes(cfg).await;
    let llm_roundtrip = llm_probe.roundtrip;
    let codex_caps = llm_probe.codex_capabilities;
    let sqlite = sqlite_diagnostics(&cfg.sqlite_path).await;
    let sqlite_ok = sqlite.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let gemini_probe = llm_probe.gemini_validation;
    let tei_model = probes.tei_info.0.as_ref().and_then(tei_model_from_info);
    let tei_summary = probes.tei_info.0.as_ref().and_then(tei_info_summary);
    let tei_dim = probes.tei_info.0.as_ref().and_then(tei_embedding_dim);
    let (chrome_ok, ref chrome_detail) = probes.chrome;
    let tei_ok = probes.tei.0;
    let qdrant_ok = probes.qdrant.0;
    let browser_runtime = build_browser_runtime(&diagnostics);

    let (vector_mode, qdrant_vector_size) =
        probe_collection_info_if_reachable(cfg, qdrant_ok, probes.client_ok).await;
    let vector_mode_str = vector_mode.as_deref();
    let vector_mode_mismatch = vector_mode_mismatch_warning(vector_mode_str, cfg);
    let dimension_mismatch = dimension_mismatch_warning(tei_dim, qdrant_vector_size);

    let services = assemble_services_map(ServicesMapInputs {
        cfg,
        sqlite,
        tei: ServiceProbeParts {
            ok: tei_ok,
            detail: probes.tei.1,
            latency_ms: probes.tei_latency_ms,
        },
        tei_model,
        tei_summary,
        qdrant_ok,
        qdrant_detail: probes.qdrant.1,
        vector_mode_str,
        vector_mode_mismatch,
        qdrant_vector_size,
        dimension_mismatch: dimension_mismatch.as_deref(),
        chrome_ok,
        chrome_detail,
        gemini_probe: &gemini_probe,
        llm_roundtrip: &llm_roundtrip,
        codex_caps,
    });

    // Cutover store-inventory: detect a non-empty or schema-incompatible store
    // and recommend `axon reset` before unified workers start. Read-only.
    let cutover_stores =
        crate::health::doctor::cutover::build_cutover_block(cfg, qdrant_ok, sqlite_ok).await;
    let reset_recommended = cutover_stores
        .get("reset_recommended")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let cutover_guidance = cutover_stores
        .get("guidance")
        .and_then(Value::as_str)
        .map(str::to_string);

    let effective_qdrant = resolve_host_endpoint(EndpointKind::Qdrant, Some(&cfg.qdrant_url), &[]);
    let effective_tei = resolve_host_endpoint(EndpointKind::Embedding, Some(&cfg.tei_url), &[]);

    let config_diagnostics = super::config_checks::run_all();

    let mut recommendations = vec![
        "CLI and MCP run all actions in-process; run `axon serve` only to expose the HTTP API."
            .to_string(),
    ];
    if let Some(guidance) = cutover_guidance {
        recommendations.push(guidance);
    }
    if !config_diagnostics.is_empty() {
        recommendations.push(format!(
            "{} config diagnostic(s) found — see config_diagnostics in this report",
            config_diagnostics.len()
        ));
    }

    Ok(serde_json::json!({
        "observed_at_utc": chrono::Utc::now().to_rfc3339(),
        "mode": {
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
        "recommendations": recommendations,
        "effective_endpoints": {
            "qdrant": effective_qdrant,
            "embedding": effective_tei,
        },
        "cutover_stores": cutover_stores,
        "reset_recommended": reset_recommended,
        "config_diagnostics": config_diagnostics,
        "services": Value::Object(services),
        "pipelines": {
            "crawl": true,
            "extract": true,
            // Readiness now reflects a real LLM round-trip (OPS-M4): a present
            // command with expired creds / unreachable endpoint reports false.
            "extract_llm_ready": llm_roundtrip.0,
            "embed": true,
            "ingest": true,
        },
        "queue_names": {},
        "browser_runtime": browser_runtime,
        "stale_jobs": 0_i64,
        "pending_jobs": pending_jobs,
        "all_ok": sqlite_ok && tei_ok && qdrant_ok && vector_mode_mismatch.is_none() && dimension_mismatch.is_none(),
    }))
}

/// Grouped `(ok, detail, latency)` for the TEI probe leg.
struct ServiceProbeParts {
    ok: bool,
    detail: Option<String>,
    latency_ms: u64,
}

/// Inputs for [`assemble_services_map`]. Extracted from `build()` to keep that
/// function under the monolith function-size cap; pure JSON assembly, no I/O.
struct ServicesMapInputs<'a> {
    cfg: &'a Config,
    sqlite: Value,
    tei: ServiceProbeParts,
    tei_model: Option<String>,
    tei_summary: Option<String>,
    qdrant_ok: bool,
    qdrant_detail: Option<String>,
    vector_mode_str: Option<&'a str>,
    vector_mode_mismatch: Option<&'a str>,
    qdrant_vector_size: Option<u64>,
    dimension_mismatch: Option<&'a str>,
    chrome_ok: bool,
    chrome_detail: &'a Option<String>,
    gemini_probe: &'a (bool, String),
    llm_roundtrip: &'a (bool, String),
    codex_caps: Option<Value>,
}

/// Build the `services` JSON object from already-collected probe results.
fn assemble_services_map(inputs: ServicesMapInputs<'_>) -> Map<String, Value> {
    let ServicesMapInputs {
        cfg,
        sqlite,
        tei,
        tei_model,
        tei_summary,
        qdrant_ok,
        qdrant_detail,
        vector_mode_str,
        vector_mode_mismatch,
        qdrant_vector_size,
        dimension_mismatch,
        chrome_ok,
        chrome_detail,
        gemini_probe,
        llm_roundtrip,
        codex_caps,
    } = inputs;

    let mut services = Map::new();
    services.insert("sqlite".to_string(), sqlite);
    services.insert(
        "tei".to_string(),
        tei_service_json(
            cfg,
            tei.ok,
            tei.detail,
            tei_model,
            tei_summary,
            tei.latency_ms,
        ),
    );
    services.insert(
        "qdrant".to_string(),
        qdrant_service_json(
            cfg,
            qdrant_ok,
            qdrant_detail,
            vector_mode_str,
            vector_mode_mismatch,
            qdrant_vector_size,
            dimension_mismatch,
        ),
    );
    services.insert(
        "chrome".to_string(),
        chrome_service_json(cfg, chrome_ok, chrome_detail),
    );
    services.insert(
        "gemini_headless".to_string(),
        gemini_service_json(cfg, gemini_probe),
    );
    services.insert(
        "llm".to_string(),
        llm_service_json(cfg, gemini_probe, llm_roundtrip),
    );
    if let Some(caps) = codex_caps {
        services.insert("codex_capabilities".to_string(), caps);
    }
    services
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
    vector_size: Option<u64>,
    dimension_mismatch: Option<&str>,
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
        "vector_size": vector_size,
        "hybrid_search_enabled": cfg.hybrid_search_enabled,
        "mode_mismatch_warning": mode_mismatch,
        "dimension_mismatch_warning": dimension_mismatch,
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

/// Backend-agnostic LLM readiness summary (OPS-M4).
///
/// Surfaces the active backend, the deep round-trip result (the authoritative
/// "can we actually synthesize" signal), and the shallow command/config
/// validation as a secondary field so an operator can distinguish "command
/// missing" from "command present but creds/endpoint broken".
fn llm_service_json(
    cfg: &Config,
    validation: &(bool, String),
    roundtrip: &(bool, String),
) -> Value {
    let backend = match cfg.llm_backend {
        crate::llm::LlmBackendKind::GeminiHeadless => "gemini-headless",
        crate::llm::LlmBackendKind::OpenAiCompat => "openai-compat",
        crate::llm::LlmBackendKind::CodexAppServer => "codex-app-server",
    };
    let model = crate::llm::configured_model_from_config(cfg);
    serde_json::json!({
        "ok": roundtrip.0,
        "backend": backend,
        "model": model,
        "roundtrip_ok": roundtrip.0,
        "roundtrip_detail": roundtrip.1,
        "config_valid": validation.0,
        "config_detail": validation.1,
    })
}

async fn probe_collection_info_if_reachable(
    cfg: &Config,
    qdrant_ok: bool,
    client_ok: bool,
) -> (Option<String>, Option<u64>) {
    if qdrant_ok && client_ok {
        probe_collection_info(&cfg.qdrant_url, &cfg.collection).await
    } else {
        (None, None)
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

/// Extract the embedding output dimension from a TEI `/info` response.
///
/// Tries several field names used across TEI versions. Returns `None` when the
/// field is absent — dimension check is silently skipped (best-effort, no false
/// positives on older TEI releases that don't expose this field).
fn tei_embedding_dim(info: &Value) -> Option<u64> {
    for key in ["embedding_dim", "dim", "hidden_size", "output_dim"] {
        if let Some(v) = info.get(key).and_then(Value::as_u64) {
            return Some(v);
        }
    }
    None
}

/// Warn when the TEI output dimension is known and differs from the Qdrant
/// collection's dense-vector size. Silently skips when either value is
/// unavailable so there are no false positives on partially-configured stacks.
fn dimension_mismatch_warning(tei_dim: Option<u64>, qdrant_size: Option<u64>) -> Option<String> {
    match (tei_dim, qdrant_size) {
        (Some(tei), Some(qdrant)) if tei != qdrant => Some(format!(
            "TEI embedding dimension ({tei}) does not match Qdrant dense-vector size ({qdrant}); \
             embed ops will fail — re-create the collection or switch TEI models to match"
        )),
        _ => None,
    }
}

/// GET `/collections/{name}`, classify the vectors block, and extract the dense vector size.
///
/// Returns `(mode, dense_size)` where:
/// - `mode` is `Some("named")`, `Some("unnamed")`, or `None` if unreachable/missing.
/// - `dense_size` is the dimension of the dense vector config:
///   unnamed → `vectors.size`; named → `vectors.dense.size`.
///   `None` when the field is absent or the collection does not exist.
///
/// Best-effort — never fails the doctor probe.
async fn probe_collection_info(
    qdrant_url: &str,
    collection: &str,
) -> (Option<String>, Option<u64>) {
    let url = format!(
        "{}/collections/{}",
        qdrant_url.trim_end_matches('/'),
        collection
    );
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return (None, None),
    };
    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(_) => return (None, None),
    };
    if !resp.status().is_success() {
        return (None, None);
    }
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return (None, None),
    };
    let vectors = match body
        .get("result")
        .and_then(|r| r.get("config"))
        .and_then(|c| c.get("params"))
        .and_then(|p| p.get("vectors"))
    {
        Some(v) => v,
        None => return (None, None),
    };

    if let Some(size) = vectors.get("size").and_then(Value::as_u64) {
        // Unnamed (legacy) collection — single flat vectors block with a `size` key.
        (Some("unnamed".to_string()), Some(size))
    } else if vectors.is_object() {
        // Named collection — dense vector lives under the "dense" key.
        let dense_size = vectors
            .get("dense")
            .and_then(|d| d.get("size"))
            .and_then(Value::as_u64);
        (Some("named".to_string()), dense_size)
    } else {
        (None, None)
    }
}

// `count_pending_jobs` moved to `jobs::store::count_pending_jobs`; the doctor
// now receives the count as a parameter so `core` no longer depends on `jobs`.
