//! OAuth 2.0 (Authorization Code + PKCE) login client for the Axon server.
//!
//! The full flow runs in the Rust shell because the webview CSP forbids
//! outbound HTTP and there is no shell/deep-link capability. Credentials are
//! cached in `OauthState` (Tauri-managed): the credential-cache lock serializes
//! token refresh (single-flight), while a separate guard serializes interactive
//! logins.

pub(crate) mod callback_server;
pub(crate) mod flow;
pub(crate) mod pkce;
pub(crate) mod secret;
pub(crate) mod status;
pub(crate) mod store;

use std::time::Duration;

use tauri::AppHandle;

use crate::axon_bridge::BridgeClient;
use crate::oauth::status::{OauthStatus, status_for};
use crate::oauth::store::StoredCredentials;
use crate::{merged_settings, validate_saved_server_url};

/// Client login timeout, kept below the server's 300s auth-request TTL so the
/// client times out first with a clear message.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(240);
/// Refresh the access token this many seconds before its stated expiry.
const EXPIRY_SKEW_SECS: i64 = 60;
/// Hard ceiling on a token refresh so a stalled `/token` can't hold the
/// credential lock (and freeze all bridge calls) indefinitely — the stream
/// client has no total request timeout.
const REFRESH_TIMEOUT: Duration = Duration::from_secs(30);
const SCOPE: &str = "axon:read axon:write";

/// Cached credentials for the current process. `Unloaded` until first access,
/// then `Loaded(Some|None)`.
enum CredCache {
    Unloaded,
    Loaded(Option<StoredCredentials>),
}

/// Tauri-managed OAuth state: the credential cache (whose lock also serializes
/// refresh — single-flight) and a guard that serializes interactive logins.
///
/// Known narrow race: an interactive login does NOT hold the cache lock during
/// the ~240s browser flow, so in a rare ordering its freshly-saved credentials
/// can be overwritten by a concurrently-completing refresh of the prior session;
/// it is self-healing on the next refresh.
pub(crate) struct OauthState {
    creds: tokio::sync::Mutex<CredCache>,
    login: tokio::sync::Mutex<()>,
}

impl OauthState {
    pub(crate) fn new() -> Self {
        OauthState {
            creds: tokio::sync::Mutex::new(CredCache::Unloaded),
            login: tokio::sync::Mutex::new(()),
        }
    }
}

impl Default for OauthState {
    fn default() -> Self {
        Self::new()
    }
}

#[tauri::command]
pub(crate) async fn axon_oauth_login(
    app: AppHandle,
    bridge: tauri::State<'_, BridgeClient>,
    oauth_state: tauri::State<'_, OauthState>,
) -> Result<OauthStatus, String> {
    // Serialize interactive logins — a second concurrent click is rejected.
    let _login_guard = oauth_state
        .login
        .try_lock()
        .map_err(|_| "a sign-in is already in progress".to_string())?;

    let settings = merged_settings(&app)?;
    let server_url = validate_saved_server_url(&settings.server_url)?;
    let client = bridge.client().clone();

    let creds = run_login(&client, &server_url).await?;
    let path = store::credentials_path(&app)?;
    store::save(&path, &creds)?;
    *oauth_state.creds.lock().await = CredCache::Loaded(Some(creds.clone()));
    Ok(status_for(Some(&creds), &server_url))
}

#[tauri::command]
pub(crate) async fn axon_oauth_logout(
    app: AppHandle,
    oauth_state: tauri::State<'_, OauthState>,
) -> Result<OauthStatus, String> {
    let path = store::credentials_path(&app)?;
    store::clear(&path)?;
    *oauth_state.creds.lock().await = CredCache::Loaded(None);
    Ok(OauthStatus::signed_out())
}

#[tauri::command]
pub(crate) async fn axon_oauth_status(
    app: AppHandle,
    oauth_state: tauri::State<'_, OauthState>,
) -> Result<OauthStatus, String> {
    let settings = merged_settings(&app)?;
    let server_url = validate_saved_server_url(&settings.server_url)?;
    let mut cache = oauth_state.creds.lock().await;
    ensure_loaded(&app, &mut cache);
    let CredCache::Loaded(slot) = &*cache else {
        unreachable!("ensure_loaded sets Loaded")
    };
    Ok(status_for(slot.as_ref(), &server_url))
}

/// The cached OAuth access token for `server_url`, refreshed if expired. Holds
/// the cache lock across any refresh so concurrent callers single-flight.
async fn effective_access_token(
    app: &AppHandle,
    client: &reqwest::Client,
    server_url: &str,
    state: &OauthState,
) -> Option<String> {
    // Defense in depth: never hand an OAuth token to a cleartext non-loopback
    // server (e.g. a hand-edited/migrated oauth.json with an http:// URL).
    if flow::require_secure_url(server_url).is_err() {
        return None;
    }
    let mut cache = state.creds.lock().await;
    ensure_loaded(app, &mut cache);
    let CredCache::Loaded(slot) = &mut *cache else {
        unreachable!("ensure_loaded sets Loaded")
    };

    // Fast path: valid cached token for this server, no refresh needed.
    {
        let creds = slot.as_ref()?;
        if !creds.matches_server(server_url) {
            return None;
        }
        if !creds.is_expired(now_unix(), EXPIRY_SKEW_SECS) {
            return Some(creds.access_token.expose().to_string());
        }
    }

    // Expired — single-flight a refresh under the held lock.
    match refresh_locked(app, client, server_url, slot).await {
        RefreshResult::Refreshed(token) => Some(token),
        RefreshResult::Cleared | RefreshResult::Kept => None,
    }
}

/// The decision a `/token` refresh yields, separated from its side effects so it
/// is unit-testable without a Tauri `AppHandle`.
enum RefreshOutcome {
    /// Success — persist, update the cache, and emit `oauth-changed`.
    Refreshed(StoredCredentials),
    /// Definitive grant rejection — wipe the session and emit `oauth-changed`.
    Cleared,
    /// Transient failure or timeout — keep the session untouched.
    Kept,
}

/// Outcome of a refresh attempt, surfaced to the bridge so a reactive 401 becomes a precise message.
enum RefreshResult {
    /// A fresh access token to retry the request with.
    Refreshed(String),
    /// The OAuth session was definitively revoked/expired and has been cleared.
    Cleared,
    /// No change — no OAuth session for this server, a transient failure, or a timeout.
    Kept,
}

/// Classify a refresh result into an outcome. Pure (no I/O, no clock).
fn classify_refresh(
    result: Result<flow::TokenResponse, flow::TokenError>,
    client_id: String,
    server_url: &str,
    token_endpoint: String,
    prior_refresh_token: Option<crate::oauth::secret::Secret>,
    now_unix: i64,
) -> RefreshOutcome {
    match result {
        Ok(token) => RefreshOutcome::Refreshed(credentials_from_token(
            client_id,
            server_url,
            token_endpoint,
            prior_refresh_token,
            token,
            now_unix,
        )),
        Err(err) if err.rejected => RefreshOutcome::Cleared,
        Err(_) => RefreshOutcome::Kept,
    }
}

/// Refresh the cached credentials under the held cache lock (single-flight).
/// Both a successful refresh (new expiry) and a *rejected* refresh (dead session
/// cleared) emit `palette://oauth-changed` so the UI re-syncs; a transient
/// failure or timeout keeps the session and emits nothing. Returns whether a new
/// token is available (`Refreshed`), the session was cleared (`Cleared`), or
/// nothing changed (`Kept`).
async fn refresh_locked(
    app: &AppHandle,
    client: &reqwest::Client,
    server_url: &str,
    slot: &mut Option<StoredCredentials>,
) -> RefreshResult {
    let snapshot = slot
        .as_ref()
        .filter(|creds| creds.matches_server(server_url))
        .and_then(|creds| {
            creds.refresh_token.as_ref().map(|rt| {
                (
                    creds.client_id.clone(),
                    creds.token_endpoint.clone(),
                    rt.expose().to_string(),
                    // Owned copy of the prior token so a refresh response that
                    // omits refresh_token can reuse it (RFC 6749 §6).
                    creds.refresh_token.clone(),
                )
            })
        });
    let Some((client_id, token_endpoint, refresh_token, prior_refresh_token)) = snapshot else {
        return RefreshResult::Kept;
    };
    let Ok(token_endpoint) = flow::require_secure_url(&token_endpoint) else {
        return RefreshResult::Kept;
    };
    let token_endpoint = token_endpoint.to_string();
    let refresh = flow::refresh_access_token(client, &token_endpoint, &client_id, &refresh_token);
    let result = match tokio::time::timeout(REFRESH_TIMEOUT, refresh).await {
        Ok(r) => r,
        Err(_) => Err(flow::TokenError {
            rejected: false,
            message: "token refresh timed out".to_string(),
        }),
    };
    match classify_refresh(
        result,
        client_id,
        server_url,
        token_endpoint,
        prior_refresh_token,
        now_unix(),
    ) {
        RefreshOutcome::Refreshed(refreshed) => {
            let access = refreshed.access_token.expose().to_string();
            with_credentials_path(app, "persist refreshed token", |path| {
                if let Err(err) = store::save(path, &refreshed) {
                    crate::diag::warn(&format!(
                        "refreshed OAuth token not persisted (will re-refresh next start): {err}"
                    ));
                }
            });
            *slot = Some(refreshed);
            emit_oauth_changed(app);
            RefreshResult::Refreshed(access)
        }
        RefreshOutcome::Cleared => {
            with_credentials_path(app, "clear dead session", |path| {
                if let Err(err) = store::clear(path) {
                    crate::diag::warn(&format!(
                        "failed to clear dead OAuth session from disk: {err}"
                    ));
                }
            });
            *slot = None;
            emit_oauth_changed(app);
            RefreshResult::Cleared
        }
        RefreshOutcome::Kept => RefreshResult::Kept,
    }
}

fn emit_oauth_changed(app: &AppHandle) {
    use tauri::Emitter;
    if let Err(err) = app.emit("palette://oauth-changed", ()) {
        crate::diag::warn(&format!("failed to emit oauth-changed event: {err}"));
    }
}

/// Resolve the credentials path and run `f` with it; log a context-tagged
/// warning (e.g. "cannot resolve oauth path to {action}") if resolution fails.
fn with_credentials_path(app: &AppHandle, action: &str, f: impl FnOnce(&std::path::Path)) {
    match store::credentials_path(app) {
        Ok(path) => f(&path),
        Err(err) => crate::diag::warn(&format!("cannot resolve oauth path to {action}: {err}")),
    }
}

/// Force a refresh regardless of apparent validity — used by the bridge on a 401
/// (proactive expiry can miss clock skew or a server-side revocation).
async fn force_refresh(
    app: &AppHandle,
    client: &reqwest::Client,
    server_url: &str,
    state: &OauthState,
) -> RefreshResult {
    if flow::require_secure_url(server_url).is_err() {
        return RefreshResult::Kept;
    }
    let mut cache = state.creds.lock().await;
    ensure_loaded(app, &mut cache);
    let CredCache::Loaded(slot) = &mut *cache else {
        unreachable!("ensure_loaded sets Loaded")
    };
    refresh_locked(app, client, server_url, slot).await
}

/// Send a bridge request with the resolved auth token; if the server answers
/// 401 and a forced refresh yields a new token, resend once. `make`
/// builds a fresh `RequestBuilder` (method/URL/headers/body) given the token to
/// attach, so the request can be rebuilt for the retry.
pub(crate) async fn send_with_reauth<F>(
    app: &AppHandle,
    client: &reqwest::Client,
    server_url: &str,
    static_token: Option<&str>,
    state: &OauthState,
    make: F,
) -> Result<reqwest::Response, String>
where
    F: Fn(Option<&str>) -> reqwest::RequestBuilder,
{
    let oauth = effective_access_token(app, client, server_url, state).await;
    let token = pick_token(oauth, static_token.map(str::to_string));
    let response = make(token.as_deref())
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        match force_refresh(app, client, server_url, state).await {
            RefreshResult::Refreshed(fresh) => {
                return make(Some(&fresh))
                    .send()
                    .await
                    .map_err(|err| err.to_string());
            }
            RefreshResult::Cleared => {
                // Session revoked/expired and cleared: fall back to a static bearer
                // token if configured, else tell the user (shown as the action error).
                if let Some(static_token) = static_token {
                    return make(Some(static_token))
                        .send()
                        .await
                        .map_err(|err| err.to_string());
                }
                return Err(
                    "Your Axon OAuth session expired or was revoked — sign in again in Settings."
                        .to_string(),
                );
            }
            RefreshResult::Kept => {}
        }
    }
    Ok(response)
}

/// Run the browser-based authorization-code flow and return fresh credentials.
async fn run_login(
    client: &reqwest::Client,
    server_url: &str,
) -> Result<StoredCredentials, String> {
    flow::require_secure_url(server_url)?;
    let meta = flow::discover(client, server_url).await?;
    let registration_endpoint = meta.registration_endpoint.clone().ok_or_else(|| {
        "this server does not support OAuth login (dynamic client registration is disabled) — \
         use a static bearer token instead"
            .to_string()
    })?;
    // Validate every server-supplied endpoint before using it.
    flow::require_secure_url(&meta.authorization_endpoint)?;
    flow::require_secure_url(&meta.token_endpoint)?;
    flow::require_secure_url(&registration_endpoint)?;

    let listener = callback_server::bind().await?;
    let client_id =
        flow::register_client(client, &registration_endpoint, &listener.redirect_uri).await?;

    let verifier = pkce::generate_code_verifier();
    let challenge = pkce::code_challenge_s256(&verifier);
    let state = pkce::generate_state();
    let authorize_url = flow::build_authorize_url(
        &meta,
        &client_id,
        &listener.redirect_uri,
        SCOPE,
        &state,
        &challenge,
    )?;

    if let Err(err) = open::that(&authorize_url) {
        return Err(format!(
            "failed to open the system browser — open this URL manually to sign in:\n{authorize_url}\n({err})"
        ));
    }

    let code = listener
        .await_code(&state, LOGIN_TIMEOUT)
        .await
        .map_err(|err| {
            format!("{err}. If the browser did not open, sign in here:\n{authorize_url}")
        })?;

    let token = flow::exchange_code(
        client,
        &meta.token_endpoint,
        &code,
        &client_id,
        &listener.redirect_uri,
        &verifier,
    )
    .await?;

    Ok(credentials_from_token(
        client_id,
        server_url,
        meta.token_endpoint,
        None,
        token,
        now_unix(),
    ))
}

fn credentials_from_token(
    client_id: String,
    server_url: &str,
    token_endpoint: String,
    prior_refresh_token: Option<crate::oauth::secret::Secret>,
    token: flow::TokenResponse,
    now_unix: i64,
) -> StoredCredentials {
    StoredCredentials {
        client_id,
        access_token: token.access_token,
        // RFC 6749 §6: a refresh response MAY omit refresh_token, meaning reuse
        // the prior one. Falling through to None would break all later refreshes.
        refresh_token: token.refresh_token.or(prior_refresh_token),
        token_endpoint,
        // Clamp so a malformed huge `expires_in` can't wrap negative (→ permanently expired).
        expires_at_unix: now_unix.saturating_add(token.expires_in.min(i64::MAX as u64) as i64),
        scope: token.scope,
        server_url: server_url.trim_end_matches('/').to_string(),
    }
}

/// Populate the cache from disk on first use. Caller holds the cache lock.
fn ensure_loaded(app: &AppHandle, cache: &mut CredCache) {
    if let CredCache::Unloaded = cache {
        let loaded = store::credentials_path(app)
            .ok()
            .and_then(|path| store::load(&path));
        *cache = CredCache::Loaded(loaded);
    }
}

/// Prefer an OAuth token over the static bearer token.
pub(crate) fn pick_token(oauth: Option<String>, static_token: Option<String>) -> Option<String> {
    oauth.or(static_token)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "oauth_tests.rs"]
mod tests;
