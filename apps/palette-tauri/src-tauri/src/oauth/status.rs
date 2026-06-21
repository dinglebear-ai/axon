//! The `OauthStatus` view-model and the `status_for` mapper that decides whether
//! the UI shows "signed in" based on whether stored credentials match the
//! currently-configured server.

use serde::Serialize;

use crate::oauth::store::StoredCredentials;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OauthStatus {
    pub signed_in: bool,
    pub scope: Option<String>,
    pub expires_at_unix: Option<i64>,
    pub server_url: Option<String>,
}

impl OauthStatus {
    /// The "not signed in to anything" status.
    pub(crate) fn signed_out() -> Self {
        OauthStatus {
            signed_in: false,
            scope: None,
            expires_at_unix: None,
            server_url: None,
        }
    }
}

/// Build a status for the UI: signed in only when the stored credentials match
/// the currently-configured server. On a server mismatch, `signed_in` is false
/// but `server_url` carries the credential's server so the UI can explain it.
pub(crate) fn status_for(creds: Option<&StoredCredentials>, current_server: &str) -> OauthStatus {
    match creds {
        Some(creds) if creds.matches_server(current_server) => OauthStatus {
            signed_in: true,
            scope: Some(creds.scope.clone()),
            expires_at_unix: Some(creds.expires_at_unix),
            server_url: Some(creds.server_url.clone()),
        },
        Some(creds) => OauthStatus {
            signed_in: false,
            scope: None,
            expires_at_unix: None,
            server_url: Some(creds.server_url.clone()),
        },
        None => OauthStatus::signed_out(),
    }
}
