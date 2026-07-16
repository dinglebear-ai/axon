use super::super::cli::{JobSubcommand, WatchSubcommand};
use super::super::types::McpTransport;
use std::env;

pub(super) const MCP_TRANSPORT_ENV: &str = "AXON_MCP_TRANSPORT";

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
    crate::paths::axon_data_base_dir().join("jobs.db")
}

pub(super) fn validate_collection_name(name: &str) -> Result<(), String> {
    crate::config::validation::validate_collection_name(name)
        .map_err(|err| format!("invalid collection name {name:?}: {err}"))
}

/// Validate `--header "K: V"` entries before they reach the request layer.
pub(super) fn validate_custom_headers(headers: Vec<String>) -> Result<Vec<String>, String> {
    crate::http::validate_custom_header_policy(&headers)?;
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

/// Parse repeatable `--budget PATH=N` entries into owned `(path, cap)` pairs
/// for spider's per-path crawl budget (bead axon_rust-37zv). Malformed entries
/// (missing `=`, empty path, non-numeric N) are skipped with a warning rather
/// than failing the whole parse. The path is used verbatim as the budget key;
/// `*` is the wildcard recognized by spider for all paths.
pub(super) fn parse_path_budgets(raw: &[String]) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    for entry in raw {
        let Some((path, cap)) = entry.rsplit_once('=') else {
            crate::logging::log_warn(&format!(
                "--budget missing '=' separator, ignoring: {entry:?}"
            ));
            continue;
        };
        let path = path.trim();
        if path.is_empty() {
            crate::logging::log_warn(&format!("--budget has empty path, ignoring: {entry:?}"));
            continue;
        }
        match cap.trim().parse::<u32>() {
            Ok(cap) => out.push((path.to_string(), cap)),
            Err(_) => crate::logging::log_warn(&format!(
                "--budget cap is not a non-negative integer, ignoring: {entry:?}"
            )),
        }
    }
    out
}

/// Resolve the MCP transport from explicit CLI flag, falling back to the
/// env override and then the command-specific default.
pub(super) fn resolve_mcp_transport(
    cli_transport: Option<McpTransport>,
    default_transport: McpTransport,
) -> McpTransport {
    if let Some(transport) = cli_transport {
        return transport;
    }
    if let Some(raw) = read_env(MCP_TRANSPORT_ENV) {
        return match raw.to_ascii_lowercase().as_str() {
            "stdio" => McpTransport::Stdio,
            "http" => McpTransport::Http,
            "both" => McpTransport::Both,
            _ => {
                eprintln!(
                    "axon: warning: unrecognized value for {MCP_TRANSPORT_ENV}={raw:?}; \
                     expected stdio/http/both. Falling back to command default."
                );
                default_transport
            }
        };
    }
    default_transport
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
            source,
            every_seconds,
            collection,
        } => {
            let mut positional = vec![
                "create".to_string(),
                source,
                "--every-seconds".to_string(),
                every_seconds.to_string(),
            ];
            if let Some(collection) = collection {
                positional.push("--collection".to_string());
                positional.push(collection);
            }
            positional
        }
        WatchSubcommand::List => vec!["list".to_string()],
        WatchSubcommand::Get { id } => vec!["get".to_string(), id],
        WatchSubcommand::Status { id } => vec!["status".to_string(), id],
        WatchSubcommand::Update {
            id,
            every_seconds,
            collection,
        } => {
            let mut positional = vec!["update".to_string(), id];
            if let Some(v) = every_seconds {
                positional.push("--every-seconds".to_string());
                positional.push(v.to_string());
            }
            if let Some(collection) = collection {
                positional.push("--collection".to_string());
                positional.push(collection);
            }
            positional
        }
        WatchSubcommand::Exec { id } => vec!["exec".to_string(), id],
        WatchSubcommand::Pause { id } => vec!["pause".to_string(), id],
        WatchSubcommand::Resume { id } => vec!["resume".to_string(), id],
        WatchSubcommand::Delete { id } => vec!["delete".to_string(), id],
        WatchSubcommand::History { id, limit } => vec![
            "history".to_string(),
            id,
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
#[path = "helpers_tests.rs"]
mod tests;
