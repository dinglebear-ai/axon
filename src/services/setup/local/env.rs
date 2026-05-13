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

pub(super) fn ensure_env_file(path: &Path) -> io::Result<EnvEnsureResult> {
    ensure_env_file_with_process(path, process_env_value)
}

fn ensure_env_file_with_process(
    path: &Path,
    process_value: impl Fn(&str) -> Option<String>,
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
    reconcile_mcp_http_token(&mut env, &process_value)?;
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

fn process_env_value(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !value.contains(['\n', '\r']))
}

pub(super) fn reconcile_mcp_http_token(
    env: &mut BTreeMap<String, String>,
    process_value: impl Fn(&str) -> Option<String>,
) -> io::Result<()> {
    if let Some(token) = process_value("AXON_MCP_HTTP_TOKEN") {
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
mod tests {
    use super::*;

    #[test]
    fn env_file_preserves_existing_secrets_and_adds_missing_runtime_keys() {
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join(".env");
        std::fs::write(
            &env_path,
            "AXON_MCP_HTTP_TOKEN=keep-me\nTAVILY_API_KEY=also-keep\n",
        )
        .unwrap();

        ensure_env_file(&env_path).unwrap();
        let raw = std::fs::read_to_string(&env_path).unwrap();
        assert!(raw.contains("AXON_MCP_HTTP_TOKEN=keep-me"));
        assert!(raw.contains("TAVILY_API_KEY=also-keep"));
        assert!(raw.contains("AXON_SERVER_URL=http://127.0.0.1:8001"));
        assert!(raw.contains("TEI_EMBEDDING_MODEL=Qwen/Qwen3-Embedding-0.6B"));
    }

    #[test]
    fn env_file_repairs_blank_token() {
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join(".env");
        std::fs::write(&env_path, "AXON_MCP_HTTP_TOKEN=   \n").unwrap();

        ensure_env_file(&env_path).unwrap();
        let raw = std::fs::read_to_string(&env_path).unwrap();
        assert!(!raw.contains("AXON_MCP_HTTP_TOKEN=   "));
        assert!(raw.contains("AXON_MCP_HTTP_TOKEN="));
    }

    #[test]
    fn parse_env_file_ignores_comments_and_empty_lines() {
        let parsed = parse_env_file("\n# comment\nA=1\nB = 'two words'\n").unwrap();
        assert_eq!(parsed.get("A").map(String::as_str), Some("1"));
        assert_eq!(parsed.get("B").map(String::as_str), Some("two words"));
    }

    #[test]
    fn parse_env_file_decodes_quoted_oauth_mode_like_runtime() {
        let parsed = parse_env_file("AXON_MCP_AUTH_MODE=\"oauth\"\n").unwrap();
        assert_eq!(
            parsed.get("AXON_MCP_AUTH_MODE").map(String::as_str),
            Some("oauth")
        );
    }

    #[test]
    fn write_env_file_quotes_dotenvy_sensitive_values() {
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join(".env");
        let env = BTreeMap::from([
            ("PLAIN".to_string(), "value".to_string()),
            ("SPACED".to_string(), "two words".to_string()),
            ("APOSTROPHE".to_string(), "Let's go".to_string()),
        ]);

        write_env_file(&env_path, &env).unwrap();

        let raw = std::fs::read_to_string(&env_path).unwrap();
        assert!(raw.contains("PLAIN=value"));
        assert!(raw.contains("SPACED='two words'"));
        assert!(raw.contains("APOSTROPHE='Let'\\''s go'"));
        let parsed = parse_env_file(&raw).unwrap();
        assert_eq!(
            parsed.get("APOSTROPHE").map(String::as_str),
            Some("Let's go")
        );
    }

    #[test]
    fn explicit_process_token_replaces_existing_env_token_only() {
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join(".env");
        std::fs::write(
            &env_path,
            "AXON_MCP_HTTP_TOKEN=old-token\nTAVILY_API_KEY=keep-this\n",
        )
        .unwrap();

        let result = ensure_env_file_with_process(&env_path, |key| {
            (key == "AXON_MCP_HTTP_TOKEN").then(|| "plugin-token".to_string())
        })
        .unwrap();
        let raw = std::fs::read_to_string(&env_path).unwrap();
        assert_eq!(
            result.values.get("AXON_MCP_HTTP_TOKEN").map(String::as_str),
            Some("plugin-token")
        );
        assert!(raw.contains("AXON_MCP_HTTP_TOKEN=plugin-token"));
        assert!(!raw.contains("old-token"));
        assert!(raw.contains("TAVILY_API_KEY=keep-this"));
        assert!(!result.phase.detail.contains("plugin-token"));
    }

    #[test]
    fn env_example_host_urls_are_loopback_not_container_dns() {
        let parsed = parse_env_file(include_str!("../../../../.env.example")).unwrap();
        for key in ["QDRANT_URL", "TEI_URL", "AXON_CHROME_REMOTE_URL"] {
            let value = parsed.get(key).expect("template key exists");
            assert!(
                value.starts_with("http://127.0.0.1:"),
                "{key} must be host loopback, got {value}"
            );
            assert!(
                !value.contains("axon-qdrant")
                    && !value.contains("axon-tei")
                    && !value.contains("axon-chrome"),
                "{key} must not use Docker DNS in host template"
            );
        }
    }
}
