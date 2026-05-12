use super::{
    DEFAULT_CHROME_URL, DEFAULT_QDRANT_URL, DEFAULT_SERVER_URL, DEFAULT_TEI_URL, LocalSetupPhase,
    LocalSetupStatus, PhaseTimer,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand_core::{OsRng, TryRngCore as _};
use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::Path;

pub(super) fn ensure_env_file(path: &Path) -> io::Result<LocalSetupPhase> {
    let timer = PhaseTimer::start("env");
    let mut env = if path.exists() {
        parse_env_file(&std::fs::read_to_string(path)?)
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
    env.entry("AXON_SERVER_URL".to_string())
        .or_insert_with(|| DEFAULT_SERVER_URL.to_string());
    env.entry("QDRANT_URL".to_string())
        .or_insert_with(|| DEFAULT_QDRANT_URL.to_string());
    env.entry("TEI_URL".to_string())
        .or_insert_with(|| DEFAULT_TEI_URL.to_string());
    env.entry("AXON_CHROME_REMOTE_URL".to_string())
        .or_insert_with(|| DEFAULT_CHROME_URL.to_string());
    env.entry("AXON_MCP_HTTP_PUBLISH".to_string())
        .or_insert_with(|| "127.0.0.1:8001".to_string());
    if !env.contains_key("AXON_MCP_HTTP_TOKEN") {
        env.insert("AXON_MCP_HTTP_TOKEN".to_string(), generate_token()?);
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
    Ok(timer.finish(
        LocalSetupStatus::Ok,
        format!("{} {} keys; added {added}", path.display(), env.len()),
    ))
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

fn parse_env_file(raw: &str) -> BTreeMap<String, String> {
    raw.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
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
        out.push_str(value);
        out.push('\n');
    }

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    io::Write::write_all(&mut options.open(path)?, out.as_bytes())?;
    Ok(())
}

fn generate_token() -> io::Result<String> {
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
    fn parse_env_file_ignores_comments_and_empty_lines() {
        let parsed = parse_env_file("\n# comment\nA=1\nB = two\n");
        assert_eq!(parsed.get("A").map(String::as_str), Some("1"));
        assert_eq!(parsed.get("B").map(String::as_str), Some("two"));
    }
}
