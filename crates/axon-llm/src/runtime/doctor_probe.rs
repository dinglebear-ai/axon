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

use crate::runtime::codex_app_server::probe_codex_capabilities;
use crate::runtime::complete_text;
use crate::runtime::headless::gemini::validate_config;

/// Fast ceiling for local/CLI-backed doctor round-trips. These providers should
/// either answer or fail quickly, so `doctor` must not inherit a long user timeout.
const FAST_LLM_PROBE_TIMEOUT_SECS: u64 = 12;

/// OpenAI-compatible endpoints can legitimately front slower remote runtimes
/// (including browser-backed providers). Give them enough time to complete a
/// real round-trip while still bounding `doctor` well below the normal 300s
/// completion timeout.
const OPENAI_COMPAT_LLM_PROBE_TIMEOUT_SECS: u64 = 90;

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
    let probe_timeout_secs =
        doctor_probe_timeout_secs(backend.kind, backend.completion_timeout_secs);
    backend.completion_timeout_secs = probe_timeout_secs;

    let req = CompletionRequest {
        system_prompt: Some("Reply with the single word: ok".to_string()),
        user_prompt: "ping".to_string(),
        model: None,
        stream: false,
        effort: None,
        backend,
    };

    let probe = complete_text(req);
    match tokio::time::timeout(Duration::from_secs(probe_timeout_secs), probe).await {
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
            format!("LLM round-trip timed out after {probe_timeout_secs}s"),
        ),
    }
}

fn doctor_probe_timeout_secs(kind: LlmBackendKind, configured_timeout_secs: u64) -> u64 {
    let ceiling = match kind {
        LlmBackendKind::OpenAiCompat => OPENAI_COMPAT_LLM_PROBE_TIMEOUT_SECS,
        LlmBackendKind::GeminiHeadless | LlmBackendKind::CodexAppServer => {
            FAST_LLM_PROBE_TIMEOUT_SECS
        }
    };
    configured_timeout_secs.clamp(1, ceiling)
}

/// Shallow gemini-headless command/config validation: `(ok, detail)`.
fn probe_gemini_headless(cfg: &Config) -> (bool, String) {
    let gemini_backend = LlmBackendConfig::from_config(cfg);
    match validate_config(&gemini_backend) {
        Ok(()) => (
            true,
            "Gemini headless command validation passed".to_string(),
        ),
        Err(err) => (false, err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_compat_probe_allows_slow_bounded_roundtrips() {
        assert_eq!(
            doctor_probe_timeout_secs(LlmBackendKind::OpenAiCompat, 300),
            OPENAI_COMPAT_LLM_PROBE_TIMEOUT_SECS
        );
    }

    #[test]
    fn openai_compat_probe_honors_shorter_configured_timeout() {
        assert_eq!(
            doctor_probe_timeout_secs(LlmBackendKind::OpenAiCompat, 45),
            45
        );
    }

    #[test]
    fn local_backends_keep_fast_probe_ceiling() {
        for kind in [
            LlmBackendKind::GeminiHeadless,
            LlmBackendKind::CodexAppServer,
        ] {
            assert_eq!(
                doctor_probe_timeout_secs(kind, 300),
                FAST_LLM_PROBE_TIMEOUT_SECS
            );
        }
    }

    #[test]
    fn probe_timeout_never_drops_below_one_second() {
        assert_eq!(
            doctor_probe_timeout_secs(LlmBackendKind::OpenAiCompat, 0),
            1
        );
    }
}
