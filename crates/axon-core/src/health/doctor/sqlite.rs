//! SQLite-runtime doctor report: SQLite + HTTP services only (no PG/Redis/AMQP probes).

use crate::config::Config;
use crate::endpoints::{EndpointKind, resolve_host_endpoint};
use crate::health::browser_diagnostics_pattern;
use crate::health::doctor::{
    LlmDoctorProbe, build_browser_runtime, tei_info_summary, tei_model_from_info,
};
use crate::sqlite::diagnostics as sqlite_diagnostics;
use serde_json::{Map, Value};
use std::error::Error;

mod provider_probes;

use provider_probes::{collect_service_probes, probe_collection_info_if_reachable};

#[cfg(test)]
use provider_probes::{
    ProbeAuth, authenticated_probe_request, probe_authenticated_tei_info, probe_collection_info,
    probe_internal_http, same_origin,
};

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
                "tier": "tier_1_source_retrieve",
                "available": qdrant_ok,
                "impact": ["source indexing, retrieve, and query require Qdrant for indexed data"],
                "remedies": if qdrant_ok { Vec::<String>::new() } else { vec!["start qdrant with `just services-up`".to_string()] },
            },
            {
                "tier": "tier_2_embedding",
                "available": tei_ok,
                "impact": ["source embedding and semantic search require TEI embeddings"],
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
            "source": true,
            "extract": true,
            // Readiness now reflects a real LLM round-trip (OPS-M4): a present
            // command with expired creds / unreachable endpoint reports false.
            "extract_llm_ready": llm_roundtrip.0,
            "watch": true,
            "prune": true,
        },
        "queue_names": {},
        "browser_runtime": browser_runtime,
        "stale_jobs": 0_i64,
        "pending_jobs": pending_jobs,
        "all_ok": sqlite_ok
            && tei_ok
            && qdrant_ok
            && chrome_ok
            && llm_roundtrip.0
            && vector_mode_mismatch.is_none()
            && dimension_mismatch.is_none(),
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
        "requested_model": model,
        "effective_model": Value::Null,
        "effective_model_source": if backend == "codex-app-server" { "not_exposed" } else { "configured" },
        "roundtrip_ok": roundtrip.0,
        "roundtrip_detail": roundtrip.1,
        "config_valid": validation.0,
        "config_detail": validation.1,
    })
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

#[cfg(test)]
#[path = "sqlite_tests.rs"]
mod tests;

// `count_pending_jobs` moved to `jobs::store::count_pending_jobs`; the doctor
// now receives the count as a parameter so `core` no longer depends on `jobs`.
