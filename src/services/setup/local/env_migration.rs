use super::{LocalSetupPhase, LocalSetupStatus, PhaseTimer};
use crate::core::config::parse::env_registry::{self, EnvClassification};
use crate::services::setup::config_store;
use std::collections::BTreeMap;
use std::io;
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
    destination: &'static str,
    value: String,
}

pub(super) fn migrate_env_file(path: &Path) -> io::Result<EnvMigrationResult> {
    let timer = PhaseTimer::start("env-migration");
    reject_shadowed_env_file(path)?;

    let raw = std::fs::read_to_string(path)?;
    let backup_path = backup_env(path)?;
    let parsed = parse_env_file_lossy(&raw);
    let mut retained = BTreeMap::new();
    let mut moved_toml = Vec::new();
    let mut report = EnvMigrationReport {
        backup_path,
        ..EnvMigrationReport::default()
    };

    for (key, value) in parsed {
        let spec = env_registry::spec_for(&key);
        match spec.map(|spec| spec.classification) {
            Some(EnvClassification::KeepEnv | EnvClassification::TrustedOperatorBootstrap) => {
                retained.insert(key, value);
                report.retained_env += 1;
            }
            Some(EnvClassification::ComposeEnv) => {
                retained.insert(key, value);
                report.compose_env += 1;
            }
            Some(EnvClassification::MoveToml) => {
                let destination = spec.and_then(|spec| spec.toml_destination).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{key} is move-toml without a TOML destination"),
                    )
                })?;
                moved_toml.push(MovedTomlValue { destination, value });
                report.moved_toml += 1;
            }
            Some(EnvClassification::Delete) => {
                report.deleted += 1;
            }
            Some(EnvClassification::CompatibilityShim) => {
                retained.insert(key, value);
                report.compatibility_shims += 1;
            }
            None => {
                report.preserved_unclassified += 1;
            }
        }
    }

    if !moved_toml.is_empty() {
        write_moved_toml_values(&moved_toml)?;
    }
    write_minimal_env(path, &retained)?;
    let values = retained;
    Ok(EnvMigrationResult {
        phase: timer.finish(LocalSetupStatus::Ok, report.detail()),
        values,
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
        set_dotted_toml_value(&mut document, moved.destination, &moved.value)?;
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
    if explicit_path != path {
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
        .as_secs();
    let backup = path.with_file_name(format!(".env.backup.{stamp}"));
    std::fs::copy(path, &backup)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(backup)
}

fn parse_env_file_lossy(raw: &str) -> BTreeMap<String, String> {
    raw.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let (key, value) = trimmed.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

fn write_minimal_env(path: &Path, env: &BTreeMap<String, String>) -> io::Result<()> {
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

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

impl EnvMigrationReport {
    fn detail(&self) -> String {
        format!(
            "backup={}; retained_env={}; compose_env={}; moved_toml={}; deleted={}; hard_defaulted={}; compatibility_shims={}; preserved_unclassified_backup_only={}",
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
