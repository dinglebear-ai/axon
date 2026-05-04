use super::super::cli::{JobSubcommand, WatchSubcommand};
use super::super::types::McpTransport;
use std::env;

/// Like `env_bool` but returns `None` when the env var is absent, empty, or unrecognized.
///
/// Recognized truthy values: `"1"`, `"true"`, `"yes"`, `"y"`, `"on"`.
/// Recognized falsy values: `"0"`, `"false"`, `"no"`, `"n"`, `"off"`.
///
/// **Unrecognized values:** warns to stderr and returns `None`, allowing the
/// TOML layer and hardcoded defaults to be consulted. A warning is emitted
/// rather than silently falling through so users can diagnose typos.
/// This runs before `init_tracing()` so `eprintln!` is intentional.
pub(crate) fn env_bool_opt(key: &str) -> Option<bool> {
    match env::var(key).ok().as_deref().map(|v| v.trim()) {
        None | Some("") => None,
        Some(v) => match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "y" | "on" => Some(true),
            "0" | "false" | "no" | "n" | "off" => Some(false),
            unrecognized => {
                eprintln!(
                    "axon: warning: unrecognized value for {key}={unrecognized:?}; \
                     expected true/false/1/0/yes/no. Falling through to TOML or default."
                );
                None
            }
        },
    }
}

/// Read an environment variable as a boolean flag with a default fallback.
///
/// Recognized truthy values: `"1"`, `"true"`, `"yes"`, `"y"`, `"on"`.
/// Recognized falsy values: `"0"`, `"false"`, `"no"`, `"n"`, `"off"`.
/// Unset, empty, or unrecognized values return `default`.
///
/// Single source of truth for boolean env-var parsing — do not duplicate.
pub(crate) fn env_bool(key: &str, default: bool) -> bool {
    env_bool_opt(key).unwrap_or(default)
}

/// Parse a comma-separated string, trimming each item and discarding empties.
/// Used for env vars like `AXON_WEB_ALLOWED_ORIGINS`, `AXON_ASK_AUTHORITATIVE_DOMAINS`, etc.
pub(super) fn parse_csv_env<F, T>(raw: &str, map_fn: F) -> Vec<T>
where
    F: Fn(&str) -> T,
{
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(map_fn)
        .collect()
}

pub(super) fn parse_origin_allowlist(raw: &str) -> Vec<String> {
    parse_csv_env(raw, ToOwned::to_owned)
}

/// Read an env var, trim it, and return `None` if missing or blank.
pub(super) fn read_env(var: &str) -> Option<String> {
    env::var(var)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(super) fn default_sqlite_path() -> std::path::PathBuf {
    crate::crates::core::paths::axon_data_base_dir()
        .join("axon")
        .join("jobs.db")
}

/// Reject collection names that would corrupt Qdrant URL paths when interpolated via
/// `format!()`. Allows letters, digits, underscore, dash, and dot; bounded to 1–255 chars.
pub(super) fn validate_collection_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("collection name must not be empty".to_string());
    }
    if name.len() > 255 {
        return Err(format!(
            "collection name too long ({} chars, max 255)",
            name.len()
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(format!(
            "collection name '{name}' contains invalid characters; \
             only [A-Za-z0-9_.-] are allowed"
        ));
    }
    // Defence-in-depth against path traversal even within the allowed charset.
    if name == "." || name == ".." || name.starts_with("..") {
        return Err(format!("collection name '{name}' is reserved"));
    }
    Ok(())
}

/// Validate `--header "K: V"` entries before they reach the request layer.
pub(super) fn validate_custom_headers(headers: Vec<String>) -> Result<Vec<String>, String> {
    for h in &headers {
        let Some((name, value)) = h.split_once(':') else {
            return Err(format!("--header missing ':' separator: {h:?}"));
        };
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() {
            return Err(format!("--header has empty name: {h:?}"));
        }
        // RFC 7230 token chars for header name.
        let name_ok = name.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    '!' | '#'
                        | '$'
                        | '%'
                        | '&'
                        | '\''
                        | '*'
                        | '+'
                        | '-'
                        | '.'
                        | '^'
                        | '_'
                        | '`'
                        | '|'
                        | '~'
                )
        });
        if !name_ok {
            return Err(format!(
                "--header name contains illegal character: {name:?} (RFC 7230 token chars only)"
            ));
        }
        if value.contains('\r') || value.contains('\n') {
            return Err(format!(
                "--header value contains CR or LF (CWE-93 header injection guard): {h:?}"
            ));
        }
    }
    Ok(headers)
}

/// Resolve the MCP transport from explicit CLI flag, falling back to the
/// command-specific default.
pub(super) fn resolve_mcp_transport(
    cli_transport: Option<McpTransport>,
    default_transport: McpTransport,
) -> McpTransport {
    if let Some(transport) = cli_transport {
        return transport;
    }
    default_transport
}

/// Resolve adapter command for ask/research ACP calls.
///
/// Priority:
/// 1. `AXON_ACP_ADAPTER_CMD` — explicit global override
/// 2. `AXON_ASK_AGENT=claude|codex|gemini` — per-agent adapter var
pub(crate) fn resolve_ask_adapter_cmd() -> Option<String> {
    read_env("AXON_ACP_ADAPTER_CMD").or_else(|| {
        let agent = env::var("AXON_ASK_AGENT").ok()?;
        let (var, default_cmd) = match agent.trim().to_lowercase().as_str() {
            "claude" => ("AXON_ACP_CLAUDE_ADAPTER_CMD", "claude-agent-acp"),
            "codex" => ("AXON_ACP_CODEX_ADAPTER_CMD", "codex-acp"),
            "gemini" => ("AXON_ACP_GEMINI_ADAPTER_CMD", "gemini"),
            _ => return None,
        };
        Some(read_env(var).unwrap_or_else(|| default_cmd.to_string()))
    })
}

/// Resolve adapter args for ask/research ACP calls (mirrors `resolve_ask_adapter_cmd`).
pub(crate) fn resolve_ask_adapter_args() -> Option<String> {
    read_env("AXON_ACP_ADAPTER_ARGS").or_else(|| {
        let agent = env::var("AXON_ASK_AGENT").ok()?;
        let (var, default_args) = match agent.trim().to_lowercase().as_str() {
            "claude" => ("AXON_ACP_CLAUDE_ADAPTER_ARGS", None),
            "codex" => ("AXON_ACP_CODEX_ADAPTER_ARGS", None),
            "gemini" => ("AXON_ACP_GEMINI_ADAPTER_ARGS", Some("--experimental-acp")),
            _ => return None,
        };
        if read_env("AXON_ACP_ADAPTER_CMD").is_some() {
            None
        } else {
            read_env(var).or_else(|| default_args.map(str::to_string))
        }
    })
}

pub(super) fn env_port(env_var: &str, default: u16) -> Result<u16, String> {
    match env::var(env_var).ok() {
        None => Ok(default),
        Some(raw) => raw
            .parse::<u16>()
            .map_err(|e| format!("invalid {env_var} '{raw}': {e}")),
    }
}

pub(super) fn positional_from_job(job: JobSubcommand) -> Vec<String> {
    match job {
        JobSubcommand::Status { job_id } => vec!["status".to_string(), job_id],
        JobSubcommand::Cancel { job_id } => vec!["cancel".to_string(), job_id],
        JobSubcommand::Errors { job_id } => vec!["errors".to_string(), job_id],
        JobSubcommand::List => vec!["list".to_string()],
        JobSubcommand::Cleanup => vec!["cleanup".to_string()],
        JobSubcommand::Clear => vec!["clear".to_string()],
        JobSubcommand::Worker => vec!["worker".to_string()],
        JobSubcommand::Recover => vec!["recover".to_string()],
    }
}

pub(super) fn positional_from_watch_subcommand(action: WatchSubcommand) -> Vec<String> {
    match action {
        WatchSubcommand::Create {
            name,
            task_type,
            every_seconds,
            task_payload,
        } => {
            let mut positional = vec![
                "create".to_string(),
                name,
                "--task-type".to_string(),
                task_type,
                "--every-seconds".to_string(),
                every_seconds.to_string(),
            ];
            if let Some(payload) = task_payload {
                positional.push("--task-payload".to_string());
                positional.push(payload);
            }
            positional
        }
        WatchSubcommand::List => vec!["list".to_string()],
        WatchSubcommand::Get { id } => vec!["get".to_string(), id],
        WatchSubcommand::Update { id, every_seconds } => {
            let mut positional = vec!["update".to_string(), id];
            if let Some(v) = every_seconds {
                positional.push("--every-seconds".to_string());
                positional.push(v.to_string());
            }
            positional
        }
        WatchSubcommand::RunNow { id } => vec!["run-now".to_string(), id],
        WatchSubcommand::Pause { id } => vec!["pause".to_string(), id],
        WatchSubcommand::Resume { id } => vec!["resume".to_string(), id],
        WatchSubcommand::Delete { id } => vec!["delete".to_string(), id],
        WatchSubcommand::History { id, limit } => vec![
            "history".to_string(),
            id,
            "--limit".to_string(),
            limit.to_string(),
        ],
        WatchSubcommand::Artifacts { run_id, limit } => vec![
            "artifacts".to_string(),
            run_id,
            "--limit".to_string(),
            limit.to_string(),
        ],
    }
}

/// Parse a viewport string like "1920x1080" into (width, height).
/// Falls back to (1920, 1080) on any parse failure.
pub(super) fn parse_viewport(s: &str) -> (u32, u32) {
    const DEFAULT: (u32, u32) = (1920, 1080);
    let Some((w, h)) = s.split_once('x') else {
        return DEFAULT;
    };
    match (w.trim().parse::<u32>(), h.trim().parse::<u32>()) {
        (Ok(w), Ok(h)) if w > 0 && h > 0 => (w, h),
        _ => DEFAULT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn validate_collection_name_accepts_normal_names() {
        for ok in ["cortex", "axon", "axon_v2", "axon-test", "Mem0.v1", "a"] {
            assert!(
                validate_collection_name(ok).is_ok(),
                "expected '{ok}' to be accepted"
            );
        }
    }

    #[test]
    fn validate_collection_name_rejects_path_traversal() {
        for bad in ["..", "../foo", "..foo", ""] {
            assert!(
                validate_collection_name(bad).is_err(),
                "expected '{bad}' to be rejected"
            );
        }
    }

    #[test]
    fn validate_collection_name_rejects_url_metacharacters() {
        for bad in [
            "foo/bar", "foo?x=1", "foo#frag", "foo bar", "foo\nbar", "foo%20",
        ] {
            assert!(
                validate_collection_name(bad).is_err(),
                "expected '{bad}' to be rejected"
            );
        }
    }

    #[test]
    fn validate_collection_name_rejects_overlong() {
        let huge = "a".repeat(256);
        assert!(validate_collection_name(&huge).is_err());
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn resolve_ask_adapter_cmd_claude_returns_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::remove_var("AXON_ACP_ADAPTER_CMD");
            env::remove_var("AXON_ACP_CLAUDE_ADAPTER_CMD");
            env::set_var("AXON_ASK_AGENT", "claude");
        }
        let result = resolve_ask_adapter_cmd();
        assert_eq!(result, Some("claude-agent-acp".to_string()));
        unsafe {
            env::remove_var("AXON_ASK_AGENT");
        }
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn resolve_ask_adapter_cmd_codex_returns_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::remove_var("AXON_ACP_ADAPTER_CMD");
            env::remove_var("AXON_ACP_CODEX_ADAPTER_CMD");
            env::set_var("AXON_ASK_AGENT", "codex");
        }
        let result = resolve_ask_adapter_cmd();
        assert_eq!(result, Some("codex-acp".to_string()));
        unsafe {
            env::remove_var("AXON_ASK_AGENT");
        }
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn resolve_ask_adapter_args_gemini_returns_experimental_flag() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::remove_var("AXON_ACP_ADAPTER_ARGS");
            env::remove_var("AXON_ACP_GEMINI_ADAPTER_ARGS");
            env::set_var("AXON_ASK_AGENT", "gemini");
        }
        let result = resolve_ask_adapter_args();
        assert_eq!(result, Some("--experimental-acp".to_string()));
        unsafe {
            env::remove_var("AXON_ASK_AGENT");
        }
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn resolve_ask_adapter_cmd_unknown_agent_returns_none() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::remove_var("AXON_ACP_ADAPTER_CMD");
            env::set_var("AXON_ASK_AGENT", "unknown-agent");
        }
        let result = resolve_ask_adapter_cmd();
        assert_eq!(result, None);
        unsafe {
            env::remove_var("AXON_ASK_AGENT");
        }
    }

    #[allow(unsafe_code)]
    #[test]
    fn env_bool_opt_returns_none_when_absent() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { env::remove_var("AXON_TEST_BOOL_OPT_ABSENT") };
        assert_eq!(env_bool_opt("AXON_TEST_BOOL_OPT_ABSENT"), None);
    }

    #[allow(unsafe_code)]
    #[test]
    fn env_bool_opt_returns_some_true_when_set() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { env::set_var("AXON_TEST_BOOL_OPT_TRUE", "true") };
        assert_eq!(env_bool_opt("AXON_TEST_BOOL_OPT_TRUE"), Some(true));
        unsafe { env::remove_var("AXON_TEST_BOOL_OPT_TRUE") };
    }

    #[allow(unsafe_code)]
    #[test]
    fn env_bool_opt_returns_some_false_when_set_to_0() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { env::set_var("AXON_TEST_BOOL_OPT_FALSE", "0") };
        assert_eq!(env_bool_opt("AXON_TEST_BOOL_OPT_FALSE"), Some(false));
        unsafe { env::remove_var("AXON_TEST_BOOL_OPT_FALSE") };
    }
}
