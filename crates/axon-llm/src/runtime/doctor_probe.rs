//! LLM legs of the `doctor` report.
//!
//! `axon-core` owns the doctor report shape but cannot execute completions
//! (the real backends live here, and `axon-llm` depends on `axon-core`). This
//! module runs the bounded LLM probes and returns an
//! [`axon_core::health::LlmDoctorProbe`] the caller injects into
//! [`axon_core::health::build_doctor_report`].

use std::time::Duration;

use axon_core::config::Config;
use axon_core::health::LlmDoctorProbe;
use axon_core::llm::{CompletionRequest, LlmBackendConfig, LlmBackendKind};

use crate::runtime::codex_app_server::{
    probe_codex_capabilities, validate_config as validate_codex_config,
};
use crate::runtime::complete_text;
use crate::runtime::headless::gemini::validate_config as validate_gemini_config;
use crate::runtime::openai_compat::validate_config as validate_openai_compat_config;

/// Hard ceiling on how long the doctor LLM round-trip is allowed to take.
/// Independent of `completion_timeout_secs` (which can be 300s) so `doctor`
/// stays fast and never hangs on an unreachable backend.
const LLM_PROBE_TIMEOUT_SECS: u64 = 12;

/// Run the doctor's LLM legs (deep round-trip, gemini command validation, and —
/// for codex — the capability probe) and return them for injection into the
/// core doctor report. All legs are non-fatal and bounded; failures degrade the
/// report instead of failing `doctor`.
pub async fn build_llm_doctor_probe(cfg: &Config) -> LlmDoctorProbe {
    let is_codex = cfg.llm_backend == LlmBackendKind::CodexAppServer;
    let backend = LlmBackendConfig::from_config(cfg);
    let codex_backend = backend.clone();
    let (roundtrip, codex_caps) = spider_join(probe_llm_roundtrip(cfg), async move {
        if is_codex {
            Some(probe_codex_capabilities(&codex_backend).await.to_json())
        } else {
            None
        }
    })
    .await;

    LlmDoctorProbe {
        roundtrip,
        config_validation: probe_backend_config(cfg),
        gemini_validation: probe_gemini_headless(cfg),
        codex_capabilities: codex_caps,
    }
}

/// Join two futures concurrently. Kept local so this crate does not take a
/// dependency on `spider` just for `tokio::join!`.
async fn spider_join<A, B>(a: A, b: B) -> (A::Output, B::Output)
where
    A: Future,
    B: Future,
{
    tokio::join!(a, b)
}

/// Deep LLM probe: attempt a minimal real completion through the configured
/// backend. Catches the common production failure the shallow command-presence
/// check misses (expired Gemini credentials, unreachable OpenAI-compatible
/// endpoint). Returns `(ok, detail)`; never panics.
async fn probe_llm_roundtrip(cfg: &Config) -> (bool, String) {
    // Build a request from cfg but clamp the per-call timeout to the probe
    // ceiling so a misconfigured long timeout can't stall the doctor.
    let mut backend = LlmBackendConfig::from_config(cfg);
    backend.completion_timeout_secs = backend
        .completion_timeout_secs
        .clamp(1, LLM_PROBE_TIMEOUT_SECS);

    let req = CompletionRequest {
        system_prompt: Some("Reply with the single word: ok".to_string()),
        user_prompt: "ping".to_string(),
        model: None,
        stream: false,
        effort: None,
        backend,
    };

    let probe = complete_text(req);
    match tokio::time::timeout(Duration::from_secs(LLM_PROBE_TIMEOUT_SECS), probe).await {
        Ok(Ok(resp)) => {
            let preview: String = resp.text.trim().chars().take(40).collect();
            (
                true,
                format!("LLM round-trip succeeded (reply: {preview:?})"),
            )
        }
        Ok(Err(err)) => {
            // Truncate so a verbose upstream body doesn't bloat the report.
            let detail: String = err.to_string().chars().take(240).collect();
            (false, format!("LLM round-trip failed: {detail}"))
        }
        Err(_) => (
            false,
            format!("LLM round-trip timed out after {LLM_PROBE_TIMEOUT_SECS}s"),
        ),
    }
}

fn probe_backend_config(cfg: &Config) -> (bool, String) {
    let backend = LlmBackendConfig::from_config(cfg);
    let (name, result) = match cfg.llm_backend {
        LlmBackendKind::GeminiHeadless => ("Gemini headless", validate_gemini_config(&backend)),
        LlmBackendKind::OpenAiCompat => {
            ("OpenAI-compatible", validate_openai_compat_config(&backend))
        }
        LlmBackendKind::CodexAppServer => ("Codex app-server", validate_codex_config(&backend)),
    };
    match result {
        Ok(()) => (true, format!("{name} configuration validation passed")),
        Err(err) => (
            false,
            format!("{name} configuration validation failed: {err}"),
        ),
    }
}

/// Gemini-specific command/config validation for the dedicated doctor service row.
fn probe_gemini_headless(cfg: &Config) -> (bool, String) {
    let backend = LlmBackendConfig::from_config(cfg);
    match validate_gemini_config(&backend) {
        Ok(()) => (
            true,
            "Gemini headless configuration validation passed".to_string(),
        ),
        Err(err) => (false, err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_compat_doctor_validation_reports_the_active_backend() {
        let cfg = Config {
            llm_backend: LlmBackendKind::OpenAiCompat,
            openai_base_url: "http://127.0.0.1:43871/v1".to_string(),
            openai_model: "chatgpt-browser-medium".to_string(),
            ..Config::default()
        };

        let (ok, detail) = probe_backend_config(&cfg);
        assert!(ok, "{detail}");
        assert!(detail.contains("OpenAI-compatible configuration validation passed"));
        assert!(!detail.contains("Gemini"));
    }

    #[test]
    fn openai_compat_doctor_validation_exposes_missing_model() {
        let cfg = Config {
            llm_backend: LlmBackendKind::OpenAiCompat,
            openai_base_url: "http://127.0.0.1:43871/v1".to_string(),
            openai_model: String::new(),
            ..Config::default()
        };

        let (ok, detail) = probe_backend_config(&cfg);
        assert!(!ok);
        assert!(detail.contains("OpenAI-compatible configuration validation failed"));
        assert!(detail.contains("AXON_SYNTHESIS_OPENAI_MODEL"));
    }
}
