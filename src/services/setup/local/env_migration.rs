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
        render_minimal_env(&retained)?;
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
    let out = render_minimal_env(env)?;
    write_private_file_atomic(path, &out)
}

fn render_minimal_env(env: &BTreeMap<String, String>) -> io::Result<String> {
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
        out.push_str(&config_store::render_env_value(value));
        out.push('\n');
    }
    Ok(out)
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
            "backup={}; retained_env={}; compose_env={}; moved_toml={}; deleted={}; compatibility_shims={}; preserved_unclassified_retained={}",
            self.backup_path.display(),
            self.retained_env,
            self.compose_env,
            self.moved_toml,
            self.deleted,
            self.compatibility_shims,
            self.preserved_unclassified
        )
    }
}

#[cfg(test)]
#[path = "env_migration_tests.rs"]
mod tests;
