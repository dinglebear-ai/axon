//! Neutral AI-session utilities shared by CLI/services.
//!
//! Session acquisition itself runs through the canonical source pipeline. This
//! module intentionally contains only reusable local validation helpers, not the
//! removed prepared-session ingest or session-watch service paths.

use anyhow::{Result, anyhow};
use axon_core::config::Config;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SessionProvider {
    Claude,
    Codex,
    Gemini,
}

impl SessionProvider {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "gemini" => Ok(Self::Gemini),
            provider => Err(anyhow!(
                "unsupported session provider `{provider}`; expected claude, codex, or gemini"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionRoots {
    pub claude_projects: PathBuf,
    pub codex_sessions: PathBuf,
    pub gemini_history: PathBuf,
    pub gemini_tmp: PathBuf,
}

impl SessionRoots {
    pub fn for_home(home: impl AsRef<Path>) -> Self {
        let home = home.as_ref();
        Self {
            claude_projects: home.join(".claude/projects"),
            codex_sessions: home.join(".codex/sessions"),
            gemini_history: home.join(".gemini/history"),
            gemini_tmp: home.join(".gemini/tmp"),
        }
    }

    pub fn from_config(_cfg: &Config) -> Result<Self> {
        Self::from_home_env()
    }

    pub fn from_home_env() -> Result<Self> {
        let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?;
        Ok(Self::for_home(PathBuf::from(home)))
    }
}

#[derive(Debug, Clone)]
pub struct ValidatedSessionPath {
    pub canonical: PathBuf,
    pub provider: SessionProvider,
    pub relative: PathBuf,
    pub basename: String,
    pub redacted_display: String,
    pub path_hash: String,
}

pub fn validate_session_file_path(
    roots: &SessionRoots,
    path: &Path,
) -> Result<ValidatedSessionPath> {
    let link_meta = std::fs::symlink_metadata(path)
        .map_err(|error| anyhow!("unsupported session file: metadata failed: {error}"))?;
    if link_meta.file_type().is_symlink() {
        return Err(anyhow!("unsupported session file: symlink rejected"));
    }
    if !link_meta.is_file() {
        return Err(anyhow!("unsupported session file: not a regular file"));
    }

    let canonical = path
        .canonicalize()
        .map_err(|error| anyhow!("unsupported session file: canonicalize failed: {error}"))?;
    reject_secret_components(&canonical)?;

    for (provider, root) in canonical_provider_roots(roots) {
        let Ok(relative) = canonical.strip_prefix(&root) else {
            continue;
        };
        if !has_supported_session_extension(provider, &canonical) {
            return Err(anyhow!("unsupported session file extension"));
        }
        let relative = relative.to_path_buf();
        let basename = canonical
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("unsupported session file: missing basename"))?
            .to_string();
        let path_hash = hash_path(&canonical);
        let redacted_display = format!("{}:{basename}:{}", provider.as_str(), &path_hash[..12]);
        return Ok(ValidatedSessionPath {
            canonical,
            provider,
            relative,
            basename,
            redacted_display,
            path_hash,
        });
    }

    Err(anyhow!("unsupported session file: outside provider roots"))
}

pub fn validate_session_source_path(
    roots: &SessionRoots,
    provider: SessionProvider,
    path: &Path,
) -> Result<PathBuf> {
    let link_meta = std::fs::symlink_metadata(path)
        .map_err(|error| anyhow!("unsupported session source: metadata failed: {error}"))?;
    if link_meta.file_type().is_symlink() {
        return Err(anyhow!("unsupported session source: symlink rejected"));
    }
    if !link_meta.is_file() && !link_meta.is_dir() {
        return Err(anyhow!(
            "unsupported session source: expected a regular file or directory"
        ));
    }

    let canonical = path
        .canonicalize()
        .map_err(|error| anyhow!("unsupported session source: canonicalize failed: {error}"))?;
    reject_secret_components(&canonical)?;

    for (root_provider, root) in canonical_provider_roots(roots) {
        if root_provider != provider || canonical.strip_prefix(&root).is_err() {
            continue;
        }
        if link_meta.is_file() && !has_supported_session_extension(provider, &canonical) {
            return Err(anyhow!("unsupported session source extension"));
        }
        return Ok(canonical);
    }

    Err(anyhow!(
        "unsupported session source: outside provider roots"
    ))
}

pub fn validate_event_path_missing_ok(
    roots: &SessionRoots,
    path: &Path,
) -> Option<SessionProvider> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical_provider_roots(roots)
        .into_iter()
        .find_map(|(provider, root)| canonical.strip_prefix(root).ok().map(|_| provider))
}

pub fn has_supported_session_extension(provider: SessionProvider, path: &Path) -> bool {
    match provider {
        SessionProvider::Claude | SessionProvider::Codex => {
            path.extension().is_some_and(|ext| ext == "jsonl")
        }
        SessionProvider::Gemini => path.extension().is_some_and(|ext| ext == "json"),
    }
}

fn canonical_provider_roots(roots: &SessionRoots) -> Vec<(SessionProvider, PathBuf)> {
    [
        (SessionProvider::Claude, &roots.claude_projects),
        (SessionProvider::Codex, &roots.codex_sessions),
        (SessionProvider::Gemini, &roots.gemini_history),
        (SessionProvider::Gemini, &roots.gemini_tmp),
    ]
    .into_iter()
    .filter_map(|(provider, root)| {
        let meta = std::fs::symlink_metadata(root).ok()?;
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return None;
        }
        root.canonicalize().ok().map(|path| (provider, path))
    })
    .collect()
}

fn reject_secret_components(path: &Path) -> Result<()> {
    for component in path.components() {
        let Component::Normal(part) = component else {
            continue;
        };
        let lower = part.to_string_lossy().to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            ".env" | "secrets" | "secret" | "token" | "tokens" | "key" | "keys" | "credentials"
        ) {
            return Err(anyhow!(
                "unsupported session file: secret-like path component"
            ));
        }
    }
    Ok(())
}

fn hash_path(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hex::encode(hasher.finalize())
}
