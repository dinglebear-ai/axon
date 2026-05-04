use crate::crates::core::paths::axon_config_path;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG: &str = include_str!("../../config.example.toml");

pub(crate) struct ConfigInit {
    pub path: PathBuf,
    pub created: bool,
}

pub(crate) fn ensure_user_config() -> io::Result<ConfigInit> {
    let path = axon_config_path().ok_or_else(|| {
        io::Error::new(
            ErrorKind::NotFound,
            "HOME is unset or invalid; cannot initialize ~/.axon/config.toml",
        )
    })?;
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("config path '{}' has no parent", path.display()),
        )
    })?;
    ensure_private_dir(parent)?;

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

pub(crate) fn read_config() -> io::Result<String> {
    let init = ensure_user_config()?;
    std::fs::read_to_string(init.path)
}

pub(crate) fn write_config(raw_toml: &str) -> io::Result<()> {
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

pub(crate) fn ensure_private_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
        let metadata = std::fs::metadata(path)?;
        let mode = metadata.permissions().mode() & 0o777;
        if mode != 0o700 {
            tracing::warn!(
                path = %path.display(),
                mode = format_args!("{mode:o}"),
                "web: tightening ~/.axon directory permissions to 0700"
            );
            std::fs::set_permissions(path, PermissionsExt::from_mode(0o700))?;
        }
        Ok(())
    }

    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)
    }
}

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
    use std::io::Write as _;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);

    let mut file = options.open(path)?;
    file.write_all(contents.as_bytes())?;
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

#[cfg(test)]
mod tests {
    #[test]
    fn invalid_toml_is_rejected_before_write() {
        let result = toml::from_str::<toml::Value>("[broken");
        assert!(result.is_err());
    }
}
