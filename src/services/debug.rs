use crate::core::config::{AskBackend, Config};
use crate::core::health::build_doctor_report;
use crate::core::logging::log_warn;
use crate::services::acp_llm::{self, AcpCompletionRequest, AcpCompletionResponse};
use crate::services::types::DebugResult;
use std::error::Error;

#[must_use = "debug_report returns a Result that should be handled"]
pub async fn debug_report(cfg: &Config, user_context: &str) -> Result<DebugResult, Box<dyn Error>> {
    let acp_adapter_cmd = cfg
        .acp_adapter_cmd
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    if cfg.ask_backend.uses_acp() && acp_adapter_cmd.is_empty() {
        return Err("AXON_ACP_ADAPTER_CMD is required for debug".into());
    }
    if cfg.ask_backend.uses_acp() && cfg.openai_model.is_empty() {
        return Err("OPENAI_MODEL is required for debug".into());
    }

    // Only explicit ACP mode warms an adapter. Headless/auto use the canonical
    // short-lived CLI path and skip ACP entirely.
    let warm = match cfg.ask_backend {
        AskBackend::Headless | AskBackend::Auto => None,
        AskBackend::Acp => match acp_llm::warm_session(cfg, None) {
            Ok(w) => Some(w),
            Err(e) => {
                log_warn(&format!(
                    "debug: warm session failed to start, using cold path: {e}"
                ));
                None
            }
        },
    };

    let doctor_report = build_doctor_report(cfg).await?;

    let prompt = format!(
        "Analyze this Axon doctor report and provide actionable troubleshooting guidance.\n\
         Prioritize root causes and concrete fix commands.\n\
         Keep it concise and operator-friendly.\n\
         Include:\n\
         1) likely root causes ordered by confidence\n\
         2) exact verification commands\n\
         3) exact remediation commands\n\
         4) what to check next if fixes fail\n\n\
         Optional operator context:\n{}\n\n\
         Doctor report JSON:\n{}",
        if user_context.is_empty() {
            "(none)"
        } else {
            user_context
        },
        serde_json::to_string_pretty(&doctor_report)?
    );

    let mut request = AcpCompletionRequest::new(prompt).system_prompt(
        "You are a senior self-hosted infrastructure debugging assistant. Be precise and avoid generic advice."
    );
    if !cfg.openai_model.trim().is_empty() {
        request = request.model(cfg.openai_model.clone());
    }
    let completion = complete_debug_text(cfg, request, warm).await?;
    let analysis = if completion.text.trim().is_empty() {
        "(no debug response)"
    } else {
        completion.text.as_str()
    };

    Ok(DebugResult {
        payload: serde_json::json!({
            "doctor_report": doctor_report,
            "llm_debug": {
                "model": if cfg.openai_model.trim().is_empty() { serde_json::Value::Null } else { serde_json::json!(&cfg.openai_model) },
                "adapter_cmd": cfg.acp_adapter_cmd,
                "analysis": analysis,
            }
        }),
    })
}

async fn complete_debug_text(
    cfg: &Config,
    request: AcpCompletionRequest,
    warm: Option<acp_llm::WarmAcpSession>,
) -> Result<AcpCompletionResponse, Box<dyn Error>> {
    #[cfg(test)]
    if cfg.ask_backend.uses_headless()
        && cfg.acp_adapter_cmd.is_none()
        && cfg.openai_model.trim().is_empty()
    {
        return Ok(AcpCompletionResponse {
            text: "debug test completion".to_string(),
            usage: None,
        });
    }

    // Warm path: WarmAcpSession is Send — await directly.
    // Cold path: acp_llm::complete_text is !Send — use spawn_blocking to keep this
    // function's future Send for web/MCP call sites that require Send + 'static.
    if let Some(w) = warm {
        w.complete_text(request).await
    } else {
        let cfg_c = cfg.clone();
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|err| format!("failed to build debug ACP runtime: {err}"))?;
            rt.block_on(acp_llm::complete_text(&cfg_c, request))
                .map_err(|err| err.to_string())
        })
        .await
        .map_err(|err| format!("failed to join debug ACP task: {err}"))?
        .map_err(|err| -> Box<dyn Error> { err.into() })
    }
}

#[cfg(test)]
mod tests {
    use super::debug_report;
    use crate::core::config::{AskBackend, Config};

    #[tokio::test]
    async fn debug_report_requires_acp_adapter_command() {
        let cfg = Config {
            openai_model: "gpt-4o-mini".to_string(),
            acp_adapter_cmd: None,
            ask_backend: AskBackend::Acp,
            ..Config::default()
        };

        let err = debug_report(&cfg, "ctx").await.expect_err("should fail");
        assert!(err.to_string().contains("AXON_ACP_ADAPTER_CMD"));
    }

    #[tokio::test]
    async fn debug_report_requires_model_name() {
        let cfg = Config {
            acp_adapter_cmd: Some("codex".to_string()),
            openai_model: String::new(),
            ask_backend: AskBackend::Acp,
            ..Config::default()
        };

        let err = debug_report(&cfg, "ctx").await.expect_err("should fail");
        assert!(err.to_string().contains("OPENAI_MODEL"));
    }

    #[tokio::test]
    async fn debug_report_headless_skips_acp_and_model_prereqs() {
        let cfg = Config {
            ask_backend: AskBackend::Headless,
            acp_adapter_cmd: None,
            openai_model: String::new(),
            ..Config::default()
        };

        assert_debug_result_or_external_headless_error(
            debug_report(&cfg, "ctx").await,
            "headless should skip ACP/model prereqs",
        );
    }

    #[tokio::test]
    async fn debug_report_auto_uses_headless_prereqs() {
        let cfg = Config {
            ask_backend: AskBackend::Auto,
            acp_adapter_cmd: None,
            openai_model: String::new(),
            ..Config::default()
        };

        assert_debug_result_or_external_headless_error(
            debug_report(&cfg, "ctx").await,
            "auto should use headless prereqs",
        );
    }

    fn assert_debug_result_or_external_headless_error(
        result: Result<crate::services::types::DebugResult, Box<dyn std::error::Error>>,
        context: &str,
    ) {
        match result {
            Ok(result) => assert!(result.payload.get("llm_debug").is_some()),
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    is_expected_external_headless_error(&msg),
                    "{context}: {msg}"
                );
            }
        }
    }

    fn is_expected_external_headless_error(msg: &str) -> bool {
        [
            "failed to spawn Gemini headless command",
            "Gemini headless exited with",
            "AXON_ASK_AGENT=gemini is unavailable for headless backend",
            "HOME is required to locate Gemini CLI auth files",
        ]
        .iter()
        .any(|expected| msg.contains(expected))
    }
}
