use crate::core::paths::axon_config_path;
use std::collections::BTreeMap;
use std::io::{self, ErrorKind, Write as _};
use std::path::{Component, Path, PathBuf};

const DEFAULT_CONFIG: &str = include_str!("../../../config.example.toml");

pub struct ConfigInit {
    pub path: PathBuf,
    pub created: bool,
}

struct ConfigPath {
    path: PathBuf,
    private_parent: bool,
}

pub fn ensure_user_config() -> io::Result<ConfigInit> {
    let resolved = resolve_setup_config_path()?;
    let path = resolved.path;
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("config path '{}' has no parent", path.display()),
        )
    })?;
    if resolved.private_parent {
        ensure_private_dir(parent)?;
    } else {
        std::fs::create_dir_all(parent)?;
    }

    if path.exists() {
        tighten_file_permissions(&path)?;
        return Ok(ConfigInit {
            path,
            created: false,
        });
    }

    match create_private_file(&path, DEFAULT_CONFIG) {
        Ok(()) => Ok(ConfigInit {
            path,
            created: true,
        }),
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            tighten_file_permissions(&path)?;
            Ok(ConfigInit {
                path,
                created: false,
            })
        }
        Err(err) => Err(err),
    }
}

pub fn read_config() -> io::Result<String> {
    let init = ensure_user_config()?;
    std::fs::read_to_string(init.path)
}

pub fn write_config(raw_toml: &str) -> io::Result<()> {
    let init = ensure_user_config()?;
    toml::from_str::<toml::Value>(raw_toml).map_err(|e| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("config TOML parse error: {e}"),
        )
    })?;
    reject_symlink(&init.path)?;
    write_private_file(&init.path, raw_toml)
}

pub fn write_remote_runtime_env(
    env_path: &Path,
    qdrant_url: &str,
    tei_url: &str,
    chrome_remote_url: &str,
) -> io::Result<PathBuf> {
    let parent = env_path.parent().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("env path '{}' has no parent", env_path.display()),
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let mut values = read_env_pairs(env_path)?;
    values.insert("QDRANT_URL".to_string(), qdrant_url.to_string());
    values.insert("TEI_URL".to_string(), tei_url.to_string());
    values.insert(
        "AXON_CHROME_REMOTE_URL".to_string(),
        chrome_remote_url.to_string(),
    );
    let contents = render_env_pairs("# Axon remote runtime env.\n", &values)?;
    write_private_file(env_path, &contents)?;
    Ok(env_path.to_path_buf())
}

fn read_env_pairs(path: &Path) -> io::Result<BTreeMap<String, String>> {
    if path.exists() {
        reject_symlink(path)?;
    }
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(err) => return Err(err),
    };
    let mut values = BTreeMap::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let trimmed = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if is_valid_env_key(key) {
            values.insert(key.to_string(), value.trim().to_string());
        }
    }
    Ok(values)
}

fn render_env_pairs(header: &str, values: &BTreeMap<String, String>) -> io::Result<String> {
    let mut out = String::from(header);
    for (key, value) in values {
        if key.contains(['\n', '\r', '=']) || value.contains(['\n', '\r']) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                format!("{key} cannot be safely written to env format"),
            ));
        }
        out.push_str(key);
        out.push('=');
        out.push_str(&render_env_value(value));
        out.push('\n');
    }
    Ok(out)
}

fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
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

fn resolve_setup_config_path() -> io::Result<ConfigPath> {
    if let Ok(value) = std::env::var("AXON_CONFIG_PATH") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
            {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("AXON_CONFIG_PATH must point to a .toml file: {trimmed:?}"),
                ));
            }
            return Ok(ConfigPath {
                path,
                private_parent: false,
            });
        }
    }

    let path = axon_config_path().ok_or_else(|| {
        io::Error::new(
            ErrorKind::NotFound,
            "HOME is unset or invalid; cannot initialize ~/.axon/config.toml",
        )
    })?;
    Ok(ConfigPath {
        path,
        private_parent: true,
    })
}

pub use crate::core::paths::ensure_private_dir;

fn create_private_file(path: &Path, contents: &str) -> io::Result<()> {
    use std::io::Write as _;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);

    let mut file = options.open(path)?;
    file.write_all(contents.as_bytes())?;
    tighten_file_permissions(path)?;
    Ok(())
}

fn write_private_file(path: &Path, contents: &str) -> io::Result<()> {
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("path '{}' has no parent", path.display()),
        )
    })?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = parent.join(format!(
        ".{}.tmp.{stamp}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("axon")
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);

    let mut file = options.open(&tmp)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)?;
    tighten_file_permissions(path)?;
    Ok(())
}

fn tighten_file_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        reject_symlink(path)?;
        std::fs::set_permissions(path, PermissionsExt::from_mode(0o600))?;
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            format!("refusing to open symlink '{}'", path.display()),
        ));
    }
    Ok(())
}

pub fn validate_remote_dir(remote_dir: &str) -> io::Result<String> {
    let raw = remote_dir.trim();
    let trimmed = raw.trim_matches('/');
    if trimmed.is_empty() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "remote_dir must not be empty",
        ));
    }
    let path = Path::new(raw);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
        || !trimmed
            .split('/')
            .all(|part| !part.is_empty() && part.chars().all(is_safe_remote_dir_char))
    {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "remote_dir must be a relative path using only letters, numbers, '.', '_', '-', and '/'",
        ));
    }
    Ok(trimmed.to_string())
}

fn is_safe_remote_dir_char(value: char) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, '.' | '_' | '-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn invalid_toml_is_rejected_before_write() {
        let result = toml::from_str::<toml::Value>("[broken");
        assert!(result.is_err());
    }

    #[test]
    fn remote_dir_rejects_parent_components() {
        assert!(validate_remote_dir("../axon").is_err());
        assert!(validate_remote_dir("/tmp/axon").is_err());
        assert_eq!(validate_remote_dir("axon-deploy").unwrap(), "axon-deploy");
    }

    #[test]
    fn remote_dir_rejects_shell_metacharacters() {
        for value in [
            "axon $(touch /tmp/pwn)",
            "axon/$(touch_pwn)",
            "axon`touch_pwn`",
            "axon\";touch pwn;#",
            "axon;touch-pwn",
            "axon deploy",
            "axon\npwn",
        ] {
            assert!(validate_remote_dir(value).is_err(), "{value:?} should fail");
        }
        assert_eq!(
            validate_remote_dir("axon-deploy/nested_1.2").unwrap(),
            "axon-deploy/nested_1.2"
        );
    }

    #[allow(unsafe_code)]
    #[test]
    fn write_remote_runtime_env_does_not_write_service_urls_to_toml() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("custom.toml");
        let env_path = dir.path().join(".env");
        let previous = std::env::var_os("AXON_CONFIG_PATH");
        unsafe {
            std::env::set_var("AXON_CONFIG_PATH", &config_path);
        }
        std::fs::write(
            &env_path,
            "TAVILY_API_KEY=secret\nAXON_MCP_HTTP_TOKEN=token\nCUSTOM_VALUE=value with spaces\n",
        )
        .unwrap();

        let written = write_remote_runtime_env(
            &env_path,
            "http://127.0.0.1:53333",
            "http://127.0.0.1:52000",
            "http://127.0.0.1:6000",
        )
        .unwrap();

        assert_eq!(written, env_path);
        let env_raw = std::fs::read_to_string(&written).unwrap();
        assert!(env_raw.contains("QDRANT_URL=http://127.0.0.1:53333"));
        assert!(env_raw.contains("TEI_URL=http://127.0.0.1:52000"));
        assert!(env_raw.contains("AXON_CHROME_REMOTE_URL=http://127.0.0.1:6000"));
        assert!(env_raw.contains("TAVILY_API_KEY=secret"));
        assert!(env_raw.contains("AXON_MCP_HTTP_TOKEN=token"));
        assert!(env_raw.contains("CUSTOM_VALUE='value with spaces'"));

        let config_raw = std::fs::read_to_string(&config_path).unwrap_or_default();
        assert!(!config_raw.contains("[services]"));
        assert!(!config_raw.contains("qdrant-url"));
        assert!(!config_raw.contains("tei-url"));

        unsafe {
            if let Some(previous) = previous {
                std::env::set_var("AXON_CONFIG_PATH", previous);
            } else {
                std::env::remove_var("AXON_CONFIG_PATH");
            }
        }
    }
}
