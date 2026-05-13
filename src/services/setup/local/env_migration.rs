use super::{LocalSetupPhase, LocalSetupStatus, PhaseTimer};
use crate::core::config::parse::env_registry::{self, EnvClassification};
use crate::services::setup::config_store;
use std::collections::BTreeMap;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub(super) struct EnvMigrationResult {
    pub phase: LocalSetupPhase,
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
struct EnvMigrationReport {
    backup_path: PathBuf,
    retained_env: usize,
    moved_toml: usize,
    compose_env: usize,
    deleted: usize,
    hard_defaulted: usize,
    compatibility_shims: usize,
    preserved_unclassified: usize,
}

struct MovedTomlValue {
    destination: String,
    value: String,
}

pub(super) fn migrate_env_file(path: &Path) -> io::Result<EnvMigrationResult> {
    let timer = PhaseTimer::start("env-migration");
    reject_symlink(path)?;
    reject_shadowed_env_file(path)?;

    let raw = std::fs::read_to_string(path)?;
    let backup_path = backup_env(path)?;
    let parsed = config_store::parse_env_pairs_from_str(&raw)?;
    let mut retained = BTreeMap::new();
    let mut moved_toml = Vec::new();
    let mut report = EnvMigrationReport::new(backup_path);

    for (key, value) in parsed {
        let classification = env_registry::spec_for(&key).map(|spec| {
            (
                spec.classification,
                spec.toml_destination.map(str::to_string),
            )
        });
        let Some((classification, toml_destination)) = classification else {
            retained.insert(key, value);
            report.preserved_unclassified += 1;
            continue;
        };

        match classification {
            EnvClassification::KeepEnv | EnvClassification::TrustedOperatorBootstrap => {
                retained.insert(key, value);
                report.retained_env += 1;
            }
            EnvClassification::ComposeEnv => {
                retained.insert(key, value);
                report.compose_env += 1;
            }
            EnvClassification::MoveToml => {
                let destination = toml_destination.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{key} is move-toml without a TOML destination"),
                    )
                })?;
                moved_toml.push(MovedTomlValue { destination, value });
                report.moved_toml += 1;
            }
            EnvClassification::Delete => {
                report.deleted += 1;
            }
            EnvClassification::CompatibilityShim => {
                retained.insert(key, value);
                report.compatibility_shims += 1;
            }
        }
    }
    super::env::reconcile_mcp_http_token(&mut retained, process_env_value)?;

    if !moved_toml.is_empty() {
        write_moved_toml_values(&moved_toml)?;
    }
    write_minimal_env(path, &retained)?;
    Ok(EnvMigrationResult {
        phase: timer.finish(LocalSetupStatus::Ok, report.detail()),
        values: retained,
    })
}

fn write_moved_toml_values(values: &[MovedTomlValue]) -> io::Result<()> {
    let raw = config_store::read_config()?;
    let mut document = raw.parse::<toml_edit::DocumentMut>().map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("config TOML parse error: {err}"),
        )
    })?;
    for moved in values {
        set_dotted_toml_value(&mut document, &moved.destination, &moved.value)?;
    }
    config_store::write_config(&document.to_string())
}

fn set_dotted_toml_value(
    document: &mut toml_edit::DocumentMut,
    destination: &str,
    raw_value: &str,
) -> io::Result<()> {
    let (section, key) = destination.rsplit_once('.').ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid TOML destination {destination:?}"),
        )
    })?;
    let table = document[section].or_insert(toml_edit::table());
    table[key] = parse_toml_value(raw_value);
    Ok(())
}

fn parse_toml_value(raw: &str) -> toml_edit::Item {
    let trimmed = raw.trim();
    if let Ok(value) = trimmed.parse::<bool>() {
        return toml_edit::value(value);
    }
    if let Ok(value) = trimmed.parse::<i64>() {
        return toml_edit::value(value);
    }
    if let Ok(value) = trimmed.parse::<f64>() {
        return toml_edit::value(value);
    }
    toml_edit::value(trimmed)
}

fn reject_shadowed_env_file(path: &Path) -> io::Result<()> {
    let Ok(explicit) = std::env::var("AXON_ENV_FILE") else {
        return Ok(());
    };
    let explicit = explicit.trim();
    if explicit.is_empty() {
        return Ok(());
    }
    let explicit_path = Path::new(explicit);
    if canonical_existing_path(explicit_path).ok() != Some(canonical_existing_path(path)?) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "AXON_ENV_FILE is set to {}; migrate that effective env file or unset AXON_ENV_FILE before migrating {}",
                explicit_path.display(),
                path.display()
            ),
        ));
    }
    Ok(())
}

fn backup_env(path: &Path) -> io::Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let backup = path.with_file_name(format!(".env.backup.{stamp}"));
    let raw = std::fs::read(path)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(&backup)?;
    file.write_all(&raw)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(backup)
}

fn write_minimal_env(path: &Path, env: &BTreeMap<String, String>) -> io::Result<()> {
    let mut out = String::from(
        "# Axon runtime env: secrets, URLs, auth, bootstrap, compose interpolation only.\n",
    );
    for (key, value) in env {
        if value.contains(['\n', '\r']) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{key} contains a newline and cannot be safely written"),
            ));
        }
        out.push_str(key);
        out.push('=');
        out.push_str(&render_env_value(value));
        out.push('\n');
    }

    write_private_file_atomic(path, &out)
}

fn process_env_value(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !value.contains(['\n', '\r']))
}

fn canonical_existing_path(path: &Path) -> io::Result<PathBuf> {
    path.canonicalize()
}

fn reject_symlink(path: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "refusing to migrate symlinked env file '{}'",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn render_env_value(value: &str) -> String {
    if value.is_empty()
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '\'' | '"' | '\\' | '$' | '`' | '#'))
    {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    } else {
        value.to_string()
    }
}

fn write_private_file_atomic(path: &Path, contents: &str) -> io::Result<()> {
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "env path has no parent"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = parent.join(format!(".env.tmp.{stamp}"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    {
        let mut file = options.open(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

impl EnvMigrationReport {
    fn new(backup_path: PathBuf) -> Self {
        Self {
            backup_path,
            ..Self::default()
        }
    }

    fn detail(&self) -> String {
        format!(
            "backup={}; retained_env={}; compose_env={}; moved_toml={}; deleted={}; hard_defaulted={}; compatibility_shims={}; preserved_unclassified_retained={}",
            self.backup_path.display(),
            self.retained_env,
            self.compose_env,
            self.moved_toml,
            self.deleted,
            self.hard_defaulted,
            self.compatibility_shims,
            self.preserved_unclassified
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[allow(unsafe_code)]
    #[test]
    fn migration_backs_up_prunes_known_stale_and_redacts_detail_values() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join(".env");
        let previous_home = std::env::var_os("HOME");
        let previous_config_path = std::env::var_os("AXON_CONFIG_PATH");
        std::fs::write(
            &env_path,
            "AXON_BATCH_QUEUE=old\nTAVILY_API_KEY=secret-value\nTEI_MAX_CLIENT_BATCH_SIZE=32\n",
        )
        .unwrap();
        unsafe {
            std::env::remove_var("AXON_ENV_FILE");
            std::env::remove_var("AXON_CONFIG_PATH");
            std::env::set_var("HOME", dir.path());
        }

        let result = migrate_env_file(&env_path).unwrap();
        assert!(result.phase.detail.contains("backup="));
        assert!(result.phase.detail.contains("deleted=1"));
        assert!(result.phase.detail.contains("moved_toml=1"));
        assert!(!result.phase.detail.contains("secret-value"));

        let raw = std::fs::read_to_string(&env_path).unwrap();
        assert!(raw.contains("TAVILY_API_KEY=secret-value"));
        assert!(raw.contains("AXON_MCP_HTTP_TOKEN="));
        assert!(!raw.contains("AXON_BATCH_QUEUE"));
        assert!(!raw.contains("TEI_MAX_CLIENT_BATCH_SIZE"));
        let config_raw = config_store::read_config().unwrap();
        assert!(config_raw.contains("max-client-batch-size = 32"));

        unsafe {
            if let Some(previous_home) = previous_home {
                std::env::set_var("HOME", previous_home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(previous_config_path) = previous_config_path {
                std::env::set_var("AXON_CONFIG_PATH", previous_config_path);
            } else {
                std::env::remove_var("AXON_CONFIG_PATH");
            }
        }
    }

    #[allow(unsafe_code)]
    #[test]
    fn migration_retains_matrix_only_runtime_keys_and_quotes_shell_sensitive_values() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join(".env");
        let previous_home = std::env::var_os("HOME");
        let previous_config_path = std::env::var_os("AXON_CONFIG_PATH");
        std::fs::write(
            &env_path,
            "GEMINI_API_KEY=gemini-secret\nAXON_MCP_HTTP_HOST=0.0.0.0\nTEI_MAX_CONCURRENT_REQUESTS=512\nUNKNOWN_KEEP='value with spaces'\n",
        )
        .unwrap();
        unsafe {
            std::env::remove_var("AXON_ENV_FILE");
            std::env::remove_var("AXON_CONFIG_PATH");
            std::env::set_var("HOME", dir.path());
        }

        let result = migrate_env_file(&env_path).unwrap();
        assert!(
            result
                .phase
                .detail
                .contains("preserved_unclassified_retained=4")
        );

        let raw = std::fs::read_to_string(&env_path).unwrap();
        assert!(raw.contains("GEMINI_API_KEY=gemini-secret"));
        assert!(raw.contains("AXON_MCP_HTTP_HOST=0.0.0.0"));
        assert!(raw.contains("TEI_MAX_CONCURRENT_REQUESTS=512"));
        assert!(raw.contains("UNKNOWN_KEEP='value with spaces'"));

        unsafe {
            if let Some(previous_home) = previous_home {
                std::env::set_var("HOME", previous_home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(previous_config_path) = previous_config_path {
                std::env::set_var("AXON_CONFIG_PATH", previous_config_path);
            } else {
                std::env::remove_var("AXON_CONFIG_PATH");
            }
        }
    }

    #[allow(unsafe_code)]
    #[test]
    fn migration_decodes_dotenv_values_and_writes_loadable_toml_only() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join(".env");
        let previous_home = std::env::var_os("HOME");
        let previous_config_path = std::env::var_os("AXON_CONFIG_PATH");
        std::fs::write(
            &env_path,
            "export TEI_MAX_CLIENT_BATCH_SIZE='32'\nAXON_ASK_CHUNK_LIMIT=\"8\"\nAXON_ASK_HYBRID_CANDIDATES=88\nAXON_LLM_COMPLETION_CONCURRENCY=3\nTAVILY_API_KEY='secret with space'\n",
        )
        .unwrap();
        unsafe {
            std::env::remove_var("AXON_ENV_FILE");
            std::env::remove_var("AXON_CONFIG_PATH");
            std::env::set_var("HOME", dir.path());
        }

        migrate_env_file(&env_path).unwrap();

        let raw_env = std::fs::read_to_string(&env_path).unwrap();
        assert!(raw_env.contains("AXON_LLM_COMPLETION_CONCURRENCY=3"));
        assert!(raw_env.contains("TAVILY_API_KEY='secret with space'"));
        assert!(!raw_env.contains("TEI_MAX_CLIENT_BATCH_SIZE"));
        assert!(!raw_env.contains("AXON_ASK_CHUNK_LIMIT"));
        assert!(!raw_env.contains("AXON_ASK_HYBRID_CANDIDATES"));

        let config_raw = config_store::read_config().unwrap();
        assert!(config_raw.contains("max-client-batch-size = 32"));
        assert!(config_raw.contains("chunk-limit = 8"));
        assert!(config_raw.contains("ask-hybrid-candidates = 88"));
        assert!(!config_raw.contains("[llm]"));
        assert!(!config_raw.contains("ask-chunk-limit"));
        crate::core::config::parse::validate_toml_config_text(&config_raw).unwrap();

        unsafe {
            if let Some(previous_home) = previous_home {
                std::env::set_var("HOME", previous_home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(previous_config_path) = previous_config_path {
                std::env::set_var("AXON_CONFIG_PATH", previous_config_path);
            } else {
                std::env::remove_var("AXON_CONFIG_PATH");
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn migration_rejects_symlinked_env_before_backup() {
        use std::os::unix::fs::symlink;

        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.env");
        let env_path = dir.path().join(".env");
        std::fs::write(&target, "TAVILY_API_KEY=secret-value\n").unwrap();
        symlink(&target, &env_path).unwrap();

        let err = migrate_env_file(&env_path).unwrap_err();
        assert!(err.to_string().contains("symlinked env file"));
        assert!(
            std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(Result::ok)
                .all(|entry| !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".env.backup."))
        );
    }

    #[allow(unsafe_code)]
    #[test]
    fn migration_rejects_shadowed_axon_env_file() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join(".env");
        std::fs::write(&env_path, "TAVILY_API_KEY=secret-value\n").unwrap();
        unsafe {
            std::env::set_var("AXON_ENV_FILE", dir.path().join("other.env"));
        }

        let err = migrate_env_file(&env_path).unwrap_err();
        assert!(err.to_string().contains("AXON_ENV_FILE is set"));

        unsafe {
            std::env::remove_var("AXON_ENV_FILE");
        }
    }
}
