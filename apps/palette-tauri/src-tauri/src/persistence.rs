//! Disk persistence for palette-local `settings.json` preferences.
//!
//! # Atomic writes
//!
//! Palette settings use an atomic rename pattern:
//! write to `<path>.tmp`, fsync, then `rename` to the target.  On Unix the
//! target file is created with mode `0o600`.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use tauri::{AppHandle, Manager};

use crate::{PaletteSettings, PartialPaletteSettings, SETTINGS_FILE};

pub(crate) fn read_settings_result(app: &AppHandle) -> Result<PartialPaletteSettings, String> {
    let path = match settings_path(app) {
        Ok(p) => p,
        Err(err) => {
            crate::diag::warn(&err.to_string());
            return Ok(PartialPaletteSettings::default());
        }
    };
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Ok(PartialPaletteSettings::default());
        }
        Err(err) => {
            return Err(format!(
                "failed to read palette settings at {}: {err}",
                path.display()
            ));
        }
    };
    parse_settings_json(&contents, &path)
}

pub(crate) fn parse_settings_json(
    contents: &str,
    path: &Path,
) -> Result<PartialPaletteSettings, String> {
    serde_json::from_str(contents).map_err(|err| {
        format!(
            "failed to parse palette settings at {}: {err}",
            path.display()
        )
    })
}

pub(crate) fn write_settings(
    app: &AppHandle,
    settings: &PaletteSettings,
) -> Result<(), Box<dyn std::error::Error>> {
    write_settings_to_path(&settings_path(app)?, settings)
}

/// Path-parameterized core of `write_settings`, split out so it is
/// unit-testable against a plain temp-dir path without needing a live
/// `AppHandle`.
///
/// # Blast-radius note (SFTP connection profiles)
///
/// `settings.json` carrying SFTP connection profiles (host/username/
/// private_key_path triples) is a machine-readable "SSH access targeting
/// map" — higher-value reconnaissance than a bare `~/.ssh/config`, since it
/// centralizes every remote host this palette can reach plus which local key
/// unlocks each one. No extra permission step is needed here, though:
/// `atomic_write` below already creates every settings write (SFTP data or
/// not) at mode `0o600` unconditionally — the same owner-only tightening
/// this note originally proposed adding conditionally already exists as the
/// file's baseline behavior. See `write_settings_is_owner_only_when_sftp_
/// connections_present`/`..._even_with_no_sftp_connections` in
/// `persistence_tests.rs`, which assert this is 0600 in both cases (not
/// conditionally).
fn write_settings_to_path(
    path: &Path,
    settings: &PaletteSettings,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(path, serde_json::to_string_pretty(settings)?.as_bytes())?;
    Ok(())
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(SETTINGS_FILE))
        .map_err(|err| format!("failed to resolve app config directory: {err}"))
}

pub(crate) fn value_for(key: &str, file_entries: &[(String, String)]) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            file_entries
                .iter()
                .find(|(entry_key, _)| entry_key == key)
                .map(|(_, value)| value.clone())
        })
}

pub(crate) fn read_default_env_entries() -> Vec<(String, String)> {
    let Some(path) = default_env_path() else {
        return Vec::new();
    };
    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(err) => {
            crate::diag::warn(&format!(
                "failed to read Axon env file at {}: {err}",
                path.display()
            ));
            return Vec::new();
        }
    };
    parse_env_entries(&contents)
}

fn default_env_path() -> Option<PathBuf> {
    std::env::var_os("AXON_ENV_PATH")
        .or_else(|| std::env::var_os("AXON_ENV_FILE"))
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".axon/.env")))
}

fn parse_env_entries(contents: &str) -> Vec<(String, String)> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            Some((key.to_string(), trim_env_value(value)))
        })
        .collect()
}

fn trim_env_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if bytes[0] == b'"' && bytes[value.len() - 1] == b'"' {
            return unescape_double_quoted(&value[1..value.len() - 1]);
        }
        if bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'' {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn unescape_double_quoted(inner: &str) -> String {
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Write `data` to `path` atomically: write to a per-write unique temp file,
/// then rename.
///
/// The temp name carries a UUID so two concurrent writers of the same `path`
/// (e.g. a login racing a refresh writing `oauth.json`) do not collide on a
/// fixed `<path>.tmp`.  If any step fails the temp file is best-effort removed
/// so unique temps don't accumulate on error.
///
/// On Unix, the temp file is created with mode `0o600` atomically via
/// `OpenOptions::mode`, so it is never world-readable even momentarily (no
/// umask window between `open` and a separate `chmod`).  On Windows no explicit
/// permission change is applied; rely on the directory ACL to restrict access.
pub(crate) fn atomic_write(path: &Path, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let write = || -> Result<(), Box<dyn std::error::Error>> {
        {
            let mut opts = fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);

            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }

            let mut file = opts.open(&tmp)?;

            use std::io::Write;
            file.write_all(data)?;
            file.sync_all()?;
        }
        fs::rename(&tmp, path)?;
        Ok(())
    };
    write().inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

#[cfg(test)]
#[path = "persistence_tests.rs"]
mod tests;
