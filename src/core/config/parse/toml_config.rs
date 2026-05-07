// Phase 1: several TomlConfig fields are parsed and validated by serde but not yet
// wired into Config (they will be connected in follow-up beads as Config fields or
// subsystem-level env reads are added). Suppress dead_code for the whole module.
#![allow(dead_code)]

use crate::core::paths::axon_config_path;
use serde::Deserialize;
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
}

// Phase 1: TEI and worker fields are parsed by serde but not yet wired into Config
// (those fields are read directly from env by their subsystems). See #![allow(dead_code)]
// at module level. Per-struct allows are omitted — the module attribute covers them.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlTeiSection {
    /// Max retry attempts per TEI request.
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
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !explicit => {
            return Ok(TomlConfig::default());
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // File exists but is unreadable — return Err so the caller can hard-fail.
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
