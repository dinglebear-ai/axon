use std::env;
use std::sync::Arc;

use crate::crates::core::config::{Config, ConfigOverrides};

use super::super::context::ExecCommandContext;
use super::types::{DirectParams, PulseChatAgent, ServiceMode, flag_opt_usize, flag_usize};

/// Build a per-request `Config` wrapped in `Arc` by applying collection + limit
/// overrides from flags.
///
/// The returned `Arc<Config>` is used in `DirectParams` so that `call_*` wrappers
/// can clone the `Arc` into `async move` blocks.  Borrows from Arc-owned data
/// (`&*cfg`) are confined to each wrapper's own state machine and do not generate
/// HRTB `for<'a> &'a Config: Send` constraints visible to `tokio::spawn`.
fn derive_cfg(context: &ExecCommandContext, flags: &serde_json::Value) -> Arc<Config> {
    let mut overrides = ConfigOverrides::default();

    if let Some(col) = flags
        .get("collection")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        overrides.collection = Some(col.to_string());
    }
    if let Some(limit) = flag_opt_usize(flags, "limit") {
        overrides.limit = Some(limit);
    }

    let mut cfg = context.cfg.apply_overrides(&overrides);

    if let Some(cmd) = env::var("AXON_ACP_ADAPTER_CMD")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        cfg.acp_adapter_cmd = Some(cmd);
    }
    if let Some(args) = env::var("AXON_ACP_ADAPTER_ARGS")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        cfg.acp_adapter_args = Some(args);
    }

    Arc::new(cfg)
}

/// Extract all parameters from `context` and `flags` into owned values before
/// any `.await`. This ensures the containing future is `Send + 'static`.
///
/// Returns `None` when `context.mode` is not a recognised `ServiceMode` —
/// callers should treat that as "not handled".
pub(super) fn extract_params(
    context: &ExecCommandContext,
    flags: &serde_json::Value,
) -> Option<DirectParams> {
    // Classify the mode synchronously.  The `&str` borrow from `.as_str()` is
    // dropped at the end of this expression — it never escapes into any async
    // state machine.
    let mode = ServiceMode::from_str(context.mode.as_str())?;

    let cfg = derive_cfg(context, flags);
    let limit = flag_usize(flags, "limit", cfg.search_limit);
    let offset = flag_usize(flags, "offset", 0);
    let max_points = flag_opt_usize(flags, "max_points");
    let agent = PulseChatAgent::from_flag(flags.get("agent").and_then(serde_json::Value::as_str));
    let session_id = flags
        .get("session_id")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let model = flags
        .get("model")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let session_mode = flags
        .get("session_mode")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let enabled_mcp_servers = flags.get("mcp_servers").and_then(|value| {
        value.as_array().map(|arr| {
            arr.iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
    });
    let blocked_mcp_tools = flags
        .get("blocked_mcp_tools")
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let assistant_mode = flags
        .get("assistant_mode")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    // ACP adapter capability flags.  These are consumed by the pulse_chat direct-service
    // path (via DirectParams → pulse_chat.rs → acp_adapter.rs) and also forwarded as
    // CLI args on subprocess paths that accept them (via ALLOWED_FLAGS in constants.rs).
    let enable_fs = flags
        .get("enable_fs")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let enable_terminal = flags
        .get("enable_terminal")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let permission_timeout_secs = flags
        .get("permission_timeout_secs")
        .and_then(|v| v.as_u64());
    let adapter_timeout_secs = flags.get("adapter_timeout_secs").and_then(|v| v.as_u64());
    Some(DirectParams {
        mode,
        input: context.input.clone(),
        cfg,
        limit,
        offset,
        max_points,
        agent,
        session_id,
        model,
        session_mode,
        enabled_mcp_servers,
        blocked_mcp_tools,
        assistant_mode,
        enable_fs,
        enable_terminal,
        permission_timeout_secs,
        adapter_timeout_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_cfg_applies_collection_override() {
        let base = Config::default();
        let context = ExecCommandContext {
            exec_id: "test".to_string(),
            mode: "query".to_string(),
            input: "test".to_string(),
            flags: serde_json::Value::Null,
            cfg: Arc::new(base),
        };
        let flags = serde_json::json!({"collection": "my_custom_col"});
        let cfg = derive_cfg(&context, &flags);
        assert_eq!(cfg.collection, "my_custom_col");
    }

    #[test]
    fn derive_cfg_ignores_empty_collection() {
        let base = Config::default();
        let original_collection = base.collection.clone();
        let context = ExecCommandContext {
            exec_id: "test".to_string(),
            mode: "query".to_string(),
            input: "test".to_string(),
            flags: serde_json::Value::Null,
            cfg: Arc::new(base),
        };
        let flags = serde_json::json!({"collection": ""});
        let cfg = derive_cfg(&context, &flags);
        assert_eq!(cfg.collection, original_collection);
    }

    #[test]
    fn extract_params_populates_limit_and_offset() {
        let base = Config::default();
        let context = ExecCommandContext {
            exec_id: "test".to_string(),
            mode: "query".to_string(),
            input: "rust async".to_string(),
            flags: serde_json::Value::Null,
            cfg: Arc::new(base),
        };
        let flags = serde_json::json!({"limit": 25, "offset": 5});
        let params = extract_params(&context, &flags).expect("query is a recognised mode");
        assert_eq!(params.limit, 25);
        assert_eq!(params.offset, 5);
        assert_eq!(params.input, "rust async");
        assert!(matches!(params.mode, ServiceMode::Query));
        assert_eq!(params.agent, PulseChatAgent::Claude);
        assert_eq!(params.session_id, None);
        assert_eq!(params.model, None);
    }

    #[test]
    fn extract_params_populates_session_id_for_pulse_chat() {
        let base = Config::default();
        let context = ExecCommandContext {
            exec_id: "test".to_string(),
            mode: "pulse_chat".to_string(),
            input: "hello".to_string(),
            flags: serde_json::Value::Null,
            cfg: Arc::new(base),
        };
        let flags = serde_json::json!({"session_id": "session-123"});
        let params = extract_params(&context, &flags).expect("pulse_chat is a recognised mode");
        assert_eq!(params.agent, PulseChatAgent::Claude);
        assert_eq!(params.session_id.as_deref(), Some("session-123"));
        assert_eq!(params.model, None);
        assert_eq!(params.session_mode, None);
    }

    #[test]
    fn extract_params_reads_codex_agent_for_pulse_chat() {
        let base = Config::default();
        let context = ExecCommandContext {
            exec_id: "test".to_string(),
            mode: "pulse_chat".to_string(),
            input: "hello".to_string(),
            flags: serde_json::Value::Null,
            cfg: Arc::new(base),
        };
        let flags = serde_json::json!({"agent": "codex"});
        let params = extract_params(&context, &flags).expect("pulse_chat is a recognised mode");
        assert_eq!(params.agent, PulseChatAgent::Codex);
        assert_eq!(params.model, None);
        assert_eq!(params.session_mode, None);
    }

    #[test]
    fn extract_params_reads_model_for_pulse_chat() {
        let base = Config::default();
        let context = ExecCommandContext {
            exec_id: "test".to_string(),
            mode: "pulse_chat".to_string(),
            input: "hello".to_string(),
            flags: serde_json::Value::Null,
            cfg: Arc::new(base),
        };
        let flags = serde_json::json!({"agent": "codex", "model": "o3"});
        let params = extract_params(&context, &flags).expect("pulse_chat is a recognised mode");
        assert_eq!(params.agent, PulseChatAgent::Codex);
        assert_eq!(params.model.as_deref(), Some("o3"));
        assert_eq!(params.session_mode, None);
    }

    #[test]
    fn extract_params_reads_gemini_agent_for_pulse_chat() {
        let base = Config::default();
        let context = ExecCommandContext {
            exec_id: "test".to_string(),
            mode: "pulse_chat".to_string(),
            input: "hello".to_string(),
            flags: serde_json::Value::Null,
            cfg: Arc::new(base),
        };
        let flags = serde_json::json!({"agent": "gemini"});
        let params = extract_params(&context, &flags).expect("pulse_chat is a recognised mode");
        assert_eq!(params.agent, PulseChatAgent::Gemini);
        assert_eq!(params.model, None);
        assert_eq!(params.session_mode, None);
    }

    #[test]
    fn extract_params_reads_gemini_agent_with_model() {
        let base = Config::default();
        let context = ExecCommandContext {
            exec_id: "test".to_string(),
            mode: "pulse_chat".to_string(),
            input: "hello".to_string(),
            flags: serde_json::Value::Null,
            cfg: Arc::new(base),
        };
        let flags = serde_json::json!({"agent": "gemini", "model": "gemini-3-pro-preview"});
        let params = extract_params(&context, &flags).expect("pulse_chat is a recognised mode");
        assert_eq!(params.agent, PulseChatAgent::Gemini);
        assert_eq!(params.model.as_deref(), Some("gemini-3-pro-preview"));
        assert_eq!(params.session_mode, None);
    }

    #[test]
    fn extract_params_returns_none_for_unknown_mode() {
        let base = Config::default();
        let context = ExecCommandContext {
            exec_id: "test".to_string(),
            mode: "unknown_mode".to_string(),
            input: "some input".to_string(),
            flags: serde_json::Value::Null,
            cfg: Arc::new(base),
        };
        let flags = serde_json::json!({});
        assert!(extract_params(&context, &flags).is_none());
    }

    #[test]
    fn extract_params_reads_assistant_mode_flag() {
        let base = Config::default();
        let context = ExecCommandContext {
            exec_id: "test".to_string(),
            mode: "pulse_chat".to_string(),
            input: "hello".to_string(),
            flags: serde_json::Value::Null,
            cfg: Arc::new(base),
        };
        let flags = serde_json::json!({"assistant_mode": true});
        let params = extract_params(&context, &flags).expect("pulse_chat is a recognised mode");
        assert!(params.assistant_mode);
    }

    #[test]
    fn extract_params_assistant_mode_defaults_false() {
        let base = Config::default();
        let context = ExecCommandContext {
            exec_id: "test".to_string(),
            mode: "pulse_chat".to_string(),
            input: "hello".to_string(),
            flags: serde_json::Value::Null,
            cfg: Arc::new(base),
        };
        let flags = serde_json::json!({});
        let params = extract_params(&context, &flags).expect("pulse_chat is a recognised mode");
        assert!(!params.assistant_mode);
    }
}
