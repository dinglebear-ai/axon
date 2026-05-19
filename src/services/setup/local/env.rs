use super::{
    DEFAULT_CHROME_URL, DEFAULT_QDRANT_URL, DEFAULT_SERVER_URL, DEFAULT_TEI_URL, LocalSetupPhase,
    LocalSetupStatus, PhaseTimer,
};
use crate::services::setup::config_store;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand_core::{OsRng, TryRngCore as _};
use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::Path;

#[derive(Debug)]
pub(super) struct EnvEnsureResult {
    pub phase: LocalSetupPhase,
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
pub struct EnvSetupOptions {
    pub mcp_host: Option<String>,
    pub mcp_port: Option<String>,
    pub auth_mode: Option<String>,
    pub mcp_token: Option<String>,
    pub oauth_public_url: Option<String>,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub auth_admin_email: Option<String>,
    pub tavily_api_key: Option<String>,
    pub github_token: Option<String>,
    pub reddit_client_id: Option<String>,
    pub reddit_client_secret: Option<String>,
}

#[cfg(test)]
pub(super) fn ensure_env_file(path: &Path) -> io::Result<EnvEnsureResult> {
    ensure_env_file_with_options(path, &EnvSetupOptions::default())
}

pub fn ensure_env_file_with_options(
    path: &Path,
    options: &EnvSetupOptions,
) -> io::Result<EnvEnsureResult> {
    ensure_env_file_with_process(path, process_env_value, options)
}

fn ensure_env_file_with_process(
    path: &Path,
    process_value: impl Fn(&str) -> Option<String>,
    options: &EnvSetupOptions,
) -> io::Result<EnvEnsureResult> {
    let timer = PhaseTimer::start("env");
    let mut env = if path.exists() {
        parse_env_file(&std::fs::read_to_string(path)?)?
    } else {
        BTreeMap::new()
    };
    let before = env.len();
    let home = path
        .parent()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "env path has no parent"))?
        .display()
        .to_string();

    env.entry("AXON_HOME".to_string()).or_insert(home.clone());
    env.entry("AXON_DATA_DIR".to_string()).or_insert(home);
    insert_process_or_default(
        &mut env,
        "AXON_SERVER_URL",
        DEFAULT_SERVER_URL,
        &process_value,
    );
    insert_process_or_default(&mut env, "QDRANT_URL", DEFAULT_QDRANT_URL, &process_value);
    insert_process_or_default(&mut env, "TEI_URL", DEFAULT_TEI_URL, &process_value);
    insert_process_or_default(
        &mut env,
        "AXON_CHROME_REMOTE_URL",
        DEFAULT_CHROME_URL,
        &process_value,
    );
    insert_process_or_default(
        &mut env,
        "AXON_MCP_HTTP_PUBLISH",
        "127.0.0.1:8001",
        &process_value,
    );
    insert_option_process_or_default(
        &mut env,
        "AXON_MCP_HTTP_HOST",
        options.mcp_host.as_deref(),
        "127.0.0.1",
        &process_value,
    );
    insert_option_process_or_default(
        &mut env,
        "AXON_MCP_HTTP_PORT",
        options.mcp_port.as_deref(),
        "8001",
        &process_value,
    );
    insert_option_process_or_default(
        &mut env,
        "AXON_MCP_AUTH_MODE",
        options.auth_mode.as_deref(),
        "bearer",
        &process_value,
    );
    apply_optional_secret(
        &mut env,
        "TAVILY_API_KEY",
        options.tavily_api_key.as_deref(),
    );
    apply_optional_secret(&mut env, "GITHUB_TOKEN", options.github_token.as_deref());
    apply_optional_secret(
        &mut env,
        "REDDIT_CLIENT_ID",
        options.reddit_client_id.as_deref(),
    );
    apply_optional_secret(
        &mut env,
        "REDDIT_CLIENT_SECRET",
        options.reddit_client_secret.as_deref(),
    );
    apply_optional_secret(
        &mut env,
        "AXON_MCP_PUBLIC_URL",
        options.oauth_public_url.as_deref(),
    );
    apply_optional_secret(
        &mut env,
        "AXON_MCP_GOOGLE_CLIENT_ID",
        options.google_client_id.as_deref(),
    );
    apply_optional_secret(
        &mut env,
        "AXON_MCP_GOOGLE_CLIENT_SECRET",
        options.google_client_secret.as_deref(),
    );
    apply_optional_secret(
        &mut env,
        "AXON_MCP_AUTH_ADMIN_EMAIL",
        options.auth_admin_email.as_deref(),
    );
    if env
        .get("AXON_MCP_AUTH_MODE")
        .is_none_or(|value| !value.trim().eq_ignore_ascii_case("oauth"))
    {
        reconcile_mcp_http_token(&mut env, options.mcp_token.as_deref(), &process_value)?;
    } else if let Some(token) = options.mcp_token.as_deref() {
        env.insert("AXON_MCP_HTTP_TOKEN".to_string(), token.to_string());
    }
    env.entry("TEI_EMBEDDING_MODEL".to_string())
        .or_insert_with(|| "Qwen/Qwen3-Embedding-0.6B".to_string());
    env.entry("TEI_HTTP_PORT".to_string())
        .or_insert_with(|| "52000".to_string());
    env.entry("NVIDIA_VISIBLE_DEVICES".to_string())
        .or_insert_with(|| "0".to_string());
    env.entry("CUDA_VISIBLE_DEVICES".to_string())
        .or_insert_with(|| "0".to_string());

    write_env_file(path, &env)?;
    let added = env.len().saturating_sub(before);
    let phase = timer.finish(
        LocalSetupStatus::Ok,
        format!("{} {} keys; added {added}", path.display(), env.len()),
    );
    Ok(EnvEnsureResult { phase, values: env })
}

fn insert_process_or_default(
    env: &mut BTreeMap<String, String>,
    key: &str,
    default: &str,
    process_value: impl Fn(&str) -> Option<String>,
) {
    if env.get(key).is_some_and(|value| !value.trim().is_empty()) {
        return;
    }
    env.insert(
        key.to_string(),
        process_value(key).unwrap_or_else(|| default.to_string()),
    );
}

fn insert_option_process_or_default(
    env: &mut BTreeMap<String, String>,
    key: &str,
    option_value: Option<&str>,
    default: &str,
    process_value: impl Fn(&str) -> Option<String>,
) {
    if let Some(value) = option_value.filter(|value| !value.trim().is_empty()) {
        env.insert(key.to_string(), value.trim().to_string());
        return;
    }
    insert_process_or_default(env, key, default, process_value);
}

fn apply_optional_secret(env: &mut BTreeMap<String, String>, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        env.insert(key.to_string(), value.trim().to_string());
    }
}

fn process_env_value(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !value.contains(['\n', '\r']))
}

pub(super) fn reconcile_mcp_http_token(
    env: &mut BTreeMap<String, String>,
    explicit_token: Option<&str>,
    process_value: impl Fn(&str) -> Option<String>,
) -> io::Result<()> {
    if let Some(token) = explicit_token.filter(|value| !value.trim().is_empty()) {
        env.insert("AXON_MCP_HTTP_TOKEN".to_string(), token.trim().to_string());
    } else if let Some(token) = process_value("AXON_MCP_HTTP_TOKEN") {
        env.insert("AXON_MCP_HTTP_TOKEN".to_string(), token);
    } else if env
        .get("AXON_MCP_HTTP_TOKEN")
        .is_none_or(|value| value.trim().is_empty())
    {
        env.insert("AXON_MCP_HTTP_TOKEN".to_string(), generate_token()?);
    }
    Ok(())
}

pub(super) fn check_env_file(path: &Path) -> LocalSetupPhase {
    let timer = PhaseTimer::start("env");
    timer.finish(
        if path.exists() {
            LocalSetupStatus::Ok
        } else {
            LocalSetupStatus::Warn
        },
        if path.exists() {
            format!("found {}", path.display())
        } else {
            format!("missing {}; run axon setup", path.display())
        },
    )
}

pub(super) fn read_env_file_values(path: &Path) -> io::Result<BTreeMap<String, String>> {
    if path.exists() {
        parse_env_file(&std::fs::read_to_string(path)?)
    } else {
        Ok(BTreeMap::new())
    }
}

fn parse_env_file(raw: &str) -> io::Result<BTreeMap<String, String>> {
    config_store::parse_env_pairs_from_str(raw)
}

fn write_env_file(path: &Path, env: &BTreeMap<String, String>) -> io::Result<()> {
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let mut out = String::new();
    out.push_str("# Axon production runtime environment.\n");
    out.push_str("# Secrets and URLs live here; non-secret tuning belongs in config.toml.\n");
    for (key, value) in env {
        out.push_str(key);
        out.push('=');
        out.push_str(&config_store::render_env_value(value));
        out.push('\n');
    }

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    io::Write::write_all(&mut options.open(path)?, out.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub(super) fn generate_token() -> io::Result<String> {
    let mut bytes = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|e| io::Error::other(format!("OsRng failed: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

#[cfg(test)]
#[path = "env_tests.rs"]
mod tests;
