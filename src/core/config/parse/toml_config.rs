use crate::core::paths::axon_config_path;
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

/// TOML configuration — tuning knobs only, safe to commit to source control.
///
/// Phase 1 scope (~15 fields across 4 sections). All fields are `Option<T>`
/// so absent keys fall through to env var and hardcoded defaults.
/// `#[serde(deny_unknown_fields)]` turns typos into parse errors rather than
/// silent ignores.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct TomlConfig {
    #[serde(default)]
    pub services: TomlServicesSection,
    #[serde(default)]
    pub search: TomlSearchSection,
    #[serde(default)]
    pub ask: TomlAskSection,
    #[serde(default)]
    pub tei: TomlTeiSection,
    #[serde(default)]
    pub workers: TomlWorkersSection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlServicesSection {
    /// Base URL of the Qdrant vector store.
    pub qdrant_url: Option<String>,
    /// Base URL of the TEI embedding service.
    pub tei_url: Option<String>,
    /// Chrome DevTools Protocol management endpoint URL.
    pub chrome_remote_url: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlSearchSection {
    pub hybrid_enabled: Option<bool>,
    /// Candidates per prefetch arm before RRF fusion (clamped 10–500).
    pub hybrid_candidates: Option<usize>,
    /// Hybrid window for the ask pipeline (clamped 10–500).
    pub ask_hybrid_candidates: Option<usize>,
    /// HNSW ef for named-mode (dense+sparse) collections (clamped 32–512).
    pub hnsw_ef: Option<usize>,
    /// HNSW ef for legacy unnamed-mode collections (clamped 16–256).
    pub hnsw_ef_legacy: Option<usize>,
    /// Qdrant collection name.
    pub collection: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlAskSection {
    /// Max chunks returned per ask query (clamped 3–40).
    pub chunk_limit: Option<usize>,
    /// Max candidate chunks fetched before scoring (clamped 8–300).
    pub candidate_limit: Option<usize>,
    /// Minimum relevance score threshold (clamped -1.0–2.0).
    pub min_relevance_score: Option<f64>,
    /// In-process document-chunk cache for the ask full-doc fetch path.
    /// Only useful in long-lived parents (`axon serve`, `axon mcp`).
    /// (bd axon_rust-pmc)
    #[serde(default)]
    pub cache: TomlAskCacheSection,
    /// Adaptive ask heuristics — currently the full-doc fetch skip gate.
    /// Opt-in until validated against the `axon evaluate` golden set.
    /// (bd axon_rust-30y)
    #[serde(default)]
    pub adaptive: TomlAskAdaptiveSection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlAskCacheSection {
    /// Enable the cache. Default: false.
    pub enabled: Option<bool>,
    /// Max bytes (summed `chunk_text` length). Default: 268_435_456 (256 MiB).
    pub max_capacity_bytes: Option<u64>,
    /// TTL in seconds. Capped at 300s. Default: 300.
    pub ttl_secs: Option<u64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlAskAdaptiveSection {
    /// Enable the adaptive full-doc fetch skip gate. Default: false (opt-in).
    pub fulldoc_skip_enabled: Option<bool>,
    /// Minimum unique URLs required in reranked top-K. Default: 3.
    pub fulldoc_skip_min_urls: Option<usize>,
    /// Minimum total chunk_text bytes summed across reranked top-K. Default: 4000.
    pub fulldoc_skip_min_chars: Option<usize>,
    /// Cosine-mode score floor offset on top of `ask_min_relevance_score`. Default: 0.15.
    pub fulldoc_skip_score_delta: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlTeiSection {
    /// Max retry attempts after the initial TEI request.
    pub max_retries: Option<usize>,
    /// Per-attempt timeout in milliseconds.
    pub request_timeout_ms: Option<u64>,
    /// Default batch size (auto-splits on HTTP 413).
    pub max_client_batch_size: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlWorkersSection {
    /// Parallel ingest worker lanes.
    pub ingest_lanes: Option<usize>,
    /// Per-document embed timeout in seconds.
    pub embed_doc_timeout_secs: Option<u64>,
    /// Crawl queue cap (0 = unlimited).
    pub max_pending_crawl_jobs: Option<usize>,
    /// Embed queue cap (0 = unlimited).
    pub max_pending_embed_jobs: Option<usize>,
    /// Extract queue cap (0 = unlimited).
    pub max_pending_extract_jobs: Option<usize>,
    /// Ingest queue cap (0 = unlimited).
    pub max_pending_ingest_jobs: Option<usize>,
}

/// Load TOML config from the first found path:
/// 1. `AXON_CONFIG_PATH` env var (if set and non-empty)
/// 2. `~/.axon/config.toml` via `axon_config_path()` (returns None when HOME unset)
/// 3. Neither found → `Ok(TomlConfig::default())` (silent)
///
/// Error policy:
/// - Default file absent → `Ok(TomlConfig::default())` (silent)
/// - Explicit `AXON_CONFIG_PATH` absent or unreadable → `Err(...)` (caller hard-fails)
/// - Default file present but unreadable → `Err(...)` for permission errors, warning + default for other I/O errors
/// - File present, parse error → `Err(...)` with path + line number (caller hard-fails)
pub(super) fn load_toml_config() -> Result<TomlConfig, String> {
    let path = resolve_config_path()?;
    let Some(resolved) = path else {
        return Ok(TomlConfig::default());
    };
    load_from_path(&resolved.path, resolved.explicit)
}

struct ResolvedConfigPath {
    path: PathBuf,
    explicit: bool,
}

fn resolve_config_path() -> Result<Option<ResolvedConfigPath>, String> {
    // Explicit override takes highest priority.
    if let Ok(v) = std::env::var("AXON_CONFIG_PATH") {
        let trimmed = v.trim().to_string();
        if !trimmed.is_empty() {
            let path = PathBuf::from(&trimmed);
            // Require .toml extension: prevents accidental probing of arbitrary
            // file paths (e.g. /etc/passwd) and keeps parse error messages
            // informative without disclosing unexpected system files.
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
            {
                return Err(format!(
                    "axon: error: AXON_CONFIG_PATH must point to a .toml file: {trimmed:?}"
                ));
            }
            return Ok(Some(ResolvedConfigPath {
                path,
                explicit: true,
            }));
        }
    }
    // Fall back to ~/.axon/config.toml (None when HOME is unset).
    Ok(axon_config_path().map(|path| ResolvedConfigPath {
        path,
        explicit: false,
    }))
}

fn load_from_path(path: &Path, explicit: bool) -> Result<TomlConfig, String> {
    // Reject symlinks: ~/.axon/config.toml controls service URLs (Qdrant,
    // TEI, Chrome CDP, OpenAI base) and ACP adapter commands. A planted
    // symlink under a permissive ~/.axon would let a local attacker
    // redirect those baseline endpoints. `read_to_string` follows symlinks
    // by default — we lstat first.
    match std::fs::symlink_metadata(path) {
        Ok(md) if md.file_type().is_symlink() => {
            return Err(format!(
                "axon: error: refusing to load symlinked config file '{}' (potential symlink attack)",
                path.display()
            ));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !explicit => {
            return Ok(TomlConfig::default());
        }
        Err(e)
            if explicit
                || matches!(
                    e.kind(),
                    std::io::ErrorKind::PermissionDenied
                        | std::io::ErrorKind::IsADirectory
                        | std::io::ErrorKind::NotADirectory
                ) =>
        {
            return Err(format!(
                "axon: error: cannot read config file '{}': {e}",
                path.display()
            ));
        }
        Err(e) => {
            eprintln!(
                "axon: warning: cannot read config file '{}': {e}",
                path.display()
            );
            return Ok(TomlConfig::default());
        }
    }

    let contents = match read_config_file_no_follow(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !explicit => {
            return Ok(TomlConfig::default());
        }
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::PermissionDenied
                    | std::io::ErrorKind::IsADirectory
                    | std::io::ErrorKind::NotADirectory
            ) =>
        {
            // File exists but is unreadable/mis-typed — return Err so the caller can hard-fail.
            // Silent fallback would hide a misconfiguration the user must fix.
            return Err(format!(
                "axon: error: cannot read config file '{}': {e}",
                path.display()
            ));
        }
        Err(e) if explicit => {
            return Err(format!(
                "axon: error: cannot read config file '{}': {e}",
                path.display()
            ));
        }
        Err(e) => {
            eprintln!(
                "axon: warning: cannot read config file '{}': {e}",
                path.display()
            );
            return Ok(TomlConfig::default());
        }
    };

    toml::from_str::<TomlConfig>(&contents).map_err(|e| {
        format!(
            "axon: error: config file '{}' has a parse error: {e}",
            path.display()
        )
    })
}

#[cfg(unix)]
fn read_config_file_no_follow(path: &Path) -> Result<String, std::io::Error> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

#[cfg(not(unix))]
fn read_config_file_no_follow(path: &Path) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}

#[cfg(test)]
pub(super) fn load_toml_config_from_str(s: &str) -> Result<TomlConfig, toml::de::Error> {
    toml::from_str(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex;
    use tempfile::NamedTempFile;

    // Serializes env-mutating tests to avoid data races on AXON_CONFIG_PATH/HOME.
    // Uses the same pattern as helpers.rs and build_config.rs ENV_LOCK.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn missing_file_returns_default() {
        let path = Path::new("/nonexistent/path/that/should/not/exist/config.toml");
        let cfg = load_from_path(path, false).unwrap();
        assert!(cfg.search.hybrid_enabled.is_none());
        assert!(cfg.ask.chunk_limit.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn load_from_path_rejects_symlinked_config() {
        // Plant a symlink at a config path pointing at a real TOML file.
        // load_from_path must refuse to follow the symlink even though
        // the target parses cleanly — a symlink under ~/.axon/ would let
        // a local attacker redirect [services] URLs / adapter cmds.
        let target = NamedTempFile::new().unwrap();
        writeln!(target.as_file(), "[ask]\nchunk-limit = 5").unwrap();
        let link =
            std::env::temp_dir().join(format!("axon-symlink-test-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(target.path(), &link).expect("create symlink");
        let result = load_from_path(&link, true);
        let _ = std::fs::remove_file(&link);
        let err = match result {
            Ok(_) => panic!("symlinked config must be rejected, got Ok"),
            Err(e) => e,
        };
        assert!(
            err.contains("symlinked config file") || err.contains("symlink attack"),
            "error should mention symlink rejection, got: {err}"
        );
    }

    #[test]
    fn valid_toml_parses_search_section() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(
            f,
            "[search]\nhybrid-enabled = false\nhybrid-candidates = 200"
        )
        .unwrap();
        let cfg = load_from_path(f.path(), false).unwrap();
        assert_eq!(cfg.search.hybrid_enabled, Some(false));
        assert_eq!(cfg.search.hybrid_candidates, Some(200));
    }

    #[test]
    fn valid_toml_parses_ask_section() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(
            f,
            "[ask]\nchunk-limit = 5\ncandidate-limit = 50\nmin-relevance-score = 0.6"
        )
        .unwrap();
        let cfg = load_from_path(f.path(), false).unwrap();
        assert_eq!(cfg.ask.chunk_limit, Some(5));
        assert_eq!(cfg.ask.candidate_limit, Some(50));
        assert!(cfg.ask.min_relevance_score.is_some());
    }

    #[test]
    fn valid_toml_parses_tei_and_workers() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "[tei]\nmax-retries = 3\n[workers]\ningest-lanes = 4").unwrap();
        let cfg = load_from_path(f.path(), false).unwrap();
        assert_eq!(cfg.tei.max_retries, Some(3));
        assert_eq!(cfg.workers.ingest_lanes, Some(4));
    }

    #[test]
    fn malformed_toml_returns_err() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "[search\nbadly_broken = !!!").unwrap();
        let result = load_from_path(f.path(), false);
        assert!(result.is_err(), "malformed TOML should return Err");
        assert!(
            result.err().unwrap().contains("parse error"),
            "error message should mention 'parse error'"
        );
    }

    #[test]
    fn load_from_path_rejects_directory_config_path() {
        let dir = tempfile::tempdir().unwrap();
        let result = load_from_path(dir.path(), false);
        let err = match result {
            Ok(_) => panic!("directory config path should hard-fail"),
            Err(e) => e,
        };
        assert!(
            err.contains("cannot read config file"),
            "error should mention unreadable config, got: {err}"
        );
    }

    #[test]
    fn load_from_path_rejects_not_a_directory_config_path() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().join("config.toml");
        let result = load_from_path(&path, false);
        let err = match result {
            Ok(_) => panic!("NotADirectory config path should hard-fail"),
            Err(e) => e,
        };
        assert!(
            err.contains("cannot read config file"),
            "error should mention unreadable config, got: {err}"
        );
    }

    #[test]
    fn unknown_field_fails_parse() {
        let result = load_toml_config_from_str("[search]\nunknown-key = true");
        assert!(
            result.is_err(),
            "deny_unknown_fields should reject unknown keys"
        );
    }

    #[test]
    fn empty_file_returns_default() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f).unwrap();
        let cfg = load_from_path(f.path(), false).unwrap();
        assert!(cfg.search.hybrid_enabled.is_none());
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn axon_config_path_env_var_overrides_home() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = std::env::var("AXON_CONFIG_PATH").ok();
        unsafe { std::env::set_var("AXON_CONFIG_PATH", "/tmp/custom_axon_config.toml") };
        let path = resolve_config_path();
        // Unconditionally restore so a panic can't contaminate other tests.
        match saved {
            Some(v) => unsafe { std::env::set_var("AXON_CONFIG_PATH", v) },
            None => unsafe { std::env::remove_var("AXON_CONFIG_PATH") },
        }
        assert_eq!(
            path.unwrap()
                .map(|resolved| (resolved.path, resolved.explicit)),
            Some((PathBuf::from("/tmp/custom_axon_config.toml"), true))
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn axon_config_path_non_toml_extension_returns_err() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = std::env::var("AXON_CONFIG_PATH").ok();
        unsafe { std::env::set_var("AXON_CONFIG_PATH", "/etc/passwd") };
        let result = resolve_config_path();
        match saved {
            Some(v) => unsafe { std::env::set_var("AXON_CONFIG_PATH", v) },
            None => unsafe { std::env::remove_var("AXON_CONFIG_PATH") },
        }
        assert!(
            result.is_err(),
            "non-.toml AXON_CONFIG_PATH should return Err"
        );
        assert!(
            result.err().unwrap().contains("AXON_CONFIG_PATH"),
            "error should mention AXON_CONFIG_PATH"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn explicit_missing_config_path_returns_err() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = std::env::var("AXON_CONFIG_PATH").ok();
        unsafe { std::env::set_var("AXON_CONFIG_PATH", "/tmp/axon-missing-config.toml") };
        let result = load_toml_config();
        match saved {
            Some(v) => unsafe { std::env::set_var("AXON_CONFIG_PATH", v) },
            None => unsafe { std::env::remove_var("AXON_CONFIG_PATH") },
        }
        assert!(
            result.is_err(),
            "explicit missing AXON_CONFIG_PATH should hard-fail"
        );
        assert!(
            result.err().unwrap().contains("cannot read config file"),
            "error should explain the config path read failure"
        );
    }
}
