//! Persistence for OAuth credentials, stored beside `settings.json` in the
//! app config dir as `oauth.json` (mode 0o600). Holds a sensitive refresh
//! token — the token fields use `Secret`, which redacts itself in `Debug`, so
//! the derived `Debug` is safe.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::oauth::secret::Secret;

const CREDENTIALS_FILE: &str = "oauth.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct StoredCredentials {
    pub client_id: String,
    pub access_token: Secret,
    #[serde(default)]
    pub refresh_token: Option<Secret>,
    /// The token endpoint discovered at login. Refresh posts here rather than
    /// reconstructing `{server_url}/token`, which breaks behind reverse proxies.
    pub token_endpoint: String,
    pub expires_at_unix: i64,
    pub scope: String,
    pub server_url: String,
}

impl StoredCredentials {
    /// True when the access token is at or past expiry once `skew_secs` of
    /// safety margin is applied.
    pub(crate) fn is_expired(&self, now_unix: i64, skew_secs: i64) -> bool {
        now_unix + skew_secs >= self.expires_at_unix
    }

    /// True when these credentials were issued for `server_url` (trailing
    /// slashes ignored on both sides).
    pub(crate) fn matches_server(&self, server_url: &str) -> bool {
        self.server_url.trim_end_matches('/') == server_url.trim_end_matches('/')
    }
}

/// Resolve the credentials file path (`<app_config_dir>/oauth.json`).
pub(crate) fn credentials_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(CREDENTIALS_FILE))
        .map_err(|err| format!("failed to resolve app config directory: {err}"))
}

/// Load credentials, returning `None` when the file is missing or unparseable
/// (a corrupt file degrades to "signed out", never a hard error). A non-missing
/// read error is logged so it is not silently indistinguishable from absence.
pub(crate) fn load(path: &Path) -> Option<StoredCredentials> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            crate::diag::warn(&format!("failed to read oauth credentials: {err}"));
            return None;
        }
    };
    match serde_json::from_str(&contents) {
        Ok(creds) => Some(creds),
        Err(err) => {
            crate::diag::warn(&format!("ignoring unparseable oauth credentials: {err}"));
            None
        }
    }
}

/// Persist credentials atomically with `0o600` perms.
pub(crate) fn save(path: &Path, creds: &StoredCredentials) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let json = serde_json::to_string_pretty(creds).map_err(|err| err.to_string())?;
    crate::persistence::atomic_write(path, json.as_bytes()).map_err(|err| err.to_string())
}

/// Remove the credentials file. Missing file is success (idempotent).
pub(crate) fn clear(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
