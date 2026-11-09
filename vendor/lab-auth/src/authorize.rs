use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect};
use axum::{Json, response::Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use crate::config::AuthConfig;
use crate::error::AuthError;
use crate::google::AuthorizeUrlRequest;
use crate::session::{append_set_cookie, build_browser_session_cookie, create_browser_session};
use crate::state::AuthState;
use crate::types::{
    AuthorizationCodeRow, AuthorizationRequestRow, AuthorizeQuery, BrowserLoginQuery,
    BrowserLoginStateRow, CallbackQuery, ClientRegistrationRequest, ClientRegistrationResponse,
    NativeAuthorizationResultRow, NativePollQuery, NativePollResponse, RegisteredClient,
};
use crate::util::{expires_at, fingerprint, now_unix, random_token};

const AUTH_REQUEST_TTL_SECS: i64 = 300;
const NATIVE_SUCCESS_PAGE: &str = r#"<!doctype html><html><body style="font-family:sans-serif;background:#07131c;color:#e6f4fb;text-align:center;padding-top:4rem"><h2>Signed in to Axon</h2><p>You can close this tab and return to the palette.</p></body></html>"#;

/// Enforces the configured email allowlist.
///
/// `email_verified` is enforced before the email comparison: without this guard,
/// an attacker who creates a Google account with someone else's address (without
/// verifying it) could bypass the allowlist.
fn check_email_allowlist(
    email: Option<&str>,
    email_verified: Option<bool>,
    allowed_emails: &[String],
) -> Result<(), AuthError> {
    if allowed_emails.is_empty() {
        return Ok(());
    }
    if email_verified != Some(true) {
        warn!("oauth callback rejected: google did not return a verified email address");
        return Err(AuthError::AuthFailed(
            "google did not return a verified email address".to_string(),
        ));
    }
    let Some(e) = email else {
        warn!("oauth callback rejected: google did not return an email address");
        return Err(AuthError::AuthFailed(
            "google did not return an email address".to_string(),
        ));
    };
    let trimmed = e.trim();
    if allowed_emails
        .iter()
        .any(|a| a.eq_ignore_ascii_case(trimmed))
    {
        return Ok(());
    }
    warn!(
        email_id = %fingerprint(trimmed),
        "oauth callback rejected: email not in allowed list"
    );
    Err(AuthError::AuthFailed(
        "google account is not permitted to access this gateway".to_string(),
    ))
}

fn granted_scope_for_oauth_user(
    config: &AuthConfig,
    email: Option<&str>,
    requested_scope: &str,
) -> String {
    if let Some(email) = email
        && email.trim().eq_ignore_ascii_case(&config.admin_email)
    {
        return config.scopes_supported.join(" ");
    }
    requested_scope.to_string()
}

pub async fn browser_login(
    State(state): State<AuthState>,
    Query(query): Query<BrowserLoginQuery>,
) -> Result<Response, AuthError> {
    state.check_authorize_rate_limit()?;
    state.ensure_pending_oauth_state_capacity().await?;
    let return_to = sanitize_return_to(&state, query.return_to.as_deref());
    let provider_code_verifier = random_token(32)?;
    let provider_code_challenge =
        URL_SAFE_NO_PAD.encode(Sha256::digest(provider_code_verifier.as_bytes()));
    let request_state = random_token(24)?;
    let oauth_state_id = fingerprint(&request_state);

    state
        .store
        .insert_browser_login_state(BrowserLoginStateRow {
            state: request_state.clone(),
            return_to: return_to.clone(),
            provider_code_verifier,
            created_at: now_unix(),
            expires_at: now_unix() + AUTH_REQUEST_TTL_SECS,
        })
        .await?;

    let location = state.google.authorize_url(&AuthorizeUrlRequest {
        state: request_state,
        scope: state.config.default_scope.clone(),
        code_challenge: provider_code_challenge,
        code_challenge_method: "S256".to_string(),
    })?;
    info!(
        oauth_state_id = %oauth_state_id,
        return_to = %return_to,
        "browser login redirected to upstream provider"
    );

    Ok((
        StatusCode::FOUND,
        [(header::LOCATION, location.to_string())],
    )
        .into_response())
}

pub async fn register_client(
    State(state): State<AuthState>,
    Json(request): Json<ClientRegistrationRequest>,
) -> Result<Json<ClientRegistrationResponse>, AuthError> {
    state.check_register_rate_limit()?;
    if request.redirect_uris.is_empty() {
        warn!("oauth register rejected: no redirect URIs provided");
        return Err(AuthError::Validation(
            "at least one redirect URI is required".to_string(),
        ));
    }
    let native_callback_endpoint = crate::metadata::native_callback_endpoint(&state);
    for redirect_uri in &request.redirect_uris {
        if redirect_uri != &native_callback_endpoint
            && !is_allowed_redirect_uri(redirect_uri, &state.config.allowed_client_redirect_uris)
        {
            warn!(
                redirect_uri = %redirect_uri,
                native_callback_endpoint = %native_callback_endpoint,
                allowed_patterns = ?state.config.allowed_client_redirect_uris,
                "oauth register rejected: redirect URI is not in the allowlist, native callback, or loopback set"
            );
            return Err(AuthError::Validation(format!(
                "redirect URI `{redirect_uri}` must target a loopback host, match the native callback endpoint, or match an allowed redirect pattern"
            )));
        }
    }

    let client = RegisteredClient {
        client_id: random_token(18)?,
        redirect_uris: request.redirect_uris,
        created_at: now_unix(),
    };
    state.store.register_client(client.clone()).await?;
    info!(
        client_id = %client.client_id,
        redirect_uri_count = client.redirect_uris.len(),
        redirect_uris = ?client.redirect_uris,
        "oauth client registration accepted"
    );
    Ok(Json(ClientRegistrationResponse {
        client_id: client.client_id,
        redirect_uris: client.redirect_uris,
        token_endpoint_auth_method: "none".to_string(),
    }))
}

pub async fn authorize(
    State(state): State<AuthState>,
    Query(query): Query<AuthorizeQuery>,
) -> Result<Response, AuthError> {
    state.check_authorize_rate_limit()?;
    state.ensure_pending_oauth_state_capacity().await?;
    validate_response_type(&query.response_type)?;
    validate_resource(&state, query.resource.as_deref())?;
    let scope = validate_scope(
        &query.scope,
        &state.config.default_scope,
        &state.config.scopes_supported,
    )?;
    let client_state_id = fingerprint(&query.state);
    info!(
        client_id = %query.client_id,
        redirect_uri = %query.redirect_uri,
        client_state_id = %client_state_id,
        requested_scope = %query.scope,
        "oauth authorize request received"
    );
    let client = state
        .store
        .find_client(&query.client_id)
        .await?
        .ok_or_else(|| {
            warn!(
                client_id = %query.client_id,
                client_state_id = %client_state_id,
                "oauth authorize rejected: unknown client_id"
            );
            AuthError::InvalidGrant("unknown client_id".to_string())
        })?;
    if !client
        .redirect_uris
        .iter()
        .any(|uri| uri == &query.redirect_uri)
    {
        warn!(
            client_id = %query.client_id,
            redirect_uri = %query.redirect_uri,
            client_state_id = %client_state_id,
            "oauth authorize rejected: redirect URI does not match registered client"
        );
        return Err(AuthError::Validation(
            "redirect_uri does not match the registered client".to_string(),
        ));
    }
    if query.code_challenge_method != "S256" {
        warn!(
            client_id = %query.client_id,
            client_state_id = %client_state_id,
            code_challenge_method = %query.code_challenge_method,
            "oauth authorize rejected: unsupported PKCE method"
        );
        return Err(AuthError::Validation(
            "code_challenge_method must be S256".to_string(),
        ));
    }

    let provider_code_verifier = random_token(32)?;
    let provider_code_challenge =
        URL_SAFE_NO_PAD.encode(Sha256::digest(provider_code_verifier.as_bytes()));
    let request_state = random_token(24)?;
    let oauth_state_id = fingerprint(&request_state);

    state
        .store
        .insert_authorization_request(AuthorizationRequestRow {
            state: request_state.clone(),
            client_id: query.client_id.clone(),
            redirect_uri: query.redirect_uri.clone(),
            client_state: query.state.clone(),
            scope: scope.clone(),
            provider_code_verifier,
            code_challenge: query.code_challenge.clone(),
            code_challenge_method: query.code_challenge_method.clone(),
            created_at: now_unix(),
            expires_at: now_unix() + AUTH_REQUEST_TTL_SECS,
        })
        .await?;

    let location = state.google.authorize_url(&AuthorizeUrlRequest {
        state: request_state,
        scope,
        code_challenge: provider_code_challenge,
        code_challenge_method: "S256".to_string(),
    })?;
    info!(
        client_id = %query.client_id,
        redirect_uri = %query.redirect_uri,
        client_state_id = %client_state_id,
        oauth_state_id = %oauth_state_id,
        provider = "google",
        "oauth authorize request redirected to upstream provider"
    );
    debug!(
        client_id = %query.client_id,
        oauth_state_id = %oauth_state_id,
        location = %location,
        "oauth authorize redirect URL generated"
    );

    Ok((
        StatusCode::FOUND,
        [(header::LOCATION, location.to_string())],
    )
        .into_response())
}

mod callback;
mod native;
mod validation;

pub use callback::callback;
use native::no_store;
pub use native::{native_callback, native_poll};
pub(crate) use validation::validate_resource;
#[cfg(test)]
use validation::{host_pattern_matches, wildcard_matches};
use validation::{
    is_allowed_redirect_uri, sanitize_return_to, validate_response_type, validate_scope,
};

#[cfg(test)]
pub mod tests;
