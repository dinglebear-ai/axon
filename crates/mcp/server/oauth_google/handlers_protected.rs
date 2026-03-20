use axum::{
    Form, Json,
    extract::State,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::Engine;
use sha2::{Digest, Sha256};
use tracing::info;
use uuid::Uuid;

use super::helpers::{
    bearer_token_from_headers, constant_time_eq, is_allowed_redirect_uri,
    normalize_loopback_redirect_uri, request_identity_from_headers, required_scopes,
    token_error_response, unauthorized_response, unix_now_secs,
};
use super::types::{
    AccessTokenRecord, AuthCodeRecord, GoogleOAuthState, OAUTH_REFRESH_TTL_SECS,
    OAuthTokenResponse, RefreshTokenRecord, TokenRequest,
};

struct AuthCodeGrantInput {
    client_id: String,
    code: String,
    redirect_uri: String,
}

struct RefreshGrantInput {
    client_id: String,
    refresh_token: String,
}

type BoxResponse = Box<Response>;

fn required_form_field(value: Option<String>, field: &'static str) -> Result<String, BoxResponse> {
    value.ok_or_else(|| {
        Box::new(token_error_response(
            "invalid_request",
            &format!("{field} is required"),
            StatusCode::BAD_REQUEST,
        ))
    })
}

fn parse_auth_code_grant_input(form: &TokenRequest) -> Result<AuthCodeGrantInput, BoxResponse> {
    let client_id = required_form_field(form.client_id.clone(), "client_id")?;
    let code = required_form_field(form.code.clone(), "code")?;
    let redirect_uri_raw = required_form_field(form.redirect_uri.clone(), "redirect_uri")?;
    let redirect_uri = normalize_loopback_redirect_uri(&redirect_uri_raw).ok_or_else(|| {
        Box::new(token_error_response(
            "invalid_request",
            "redirect_uri is invalid",
            StatusCode::BAD_REQUEST,
        ))
    })?;
    Ok(AuthCodeGrantInput {
        client_id,
        code,
        redirect_uri,
    })
}

fn validate_redirect_policy(
    state: &GoogleOAuthState,
    redirect_uri: &str,
) -> Result<(), BoxResponse> {
    let cfg = state.config().map_err(|_| {
        Box::new(token_error_response(
            "server_error",
            "oauth configuration unavailable",
            StatusCode::INTERNAL_SERVER_ERROR,
        ))
    })?;
    if !is_allowed_redirect_uri(redirect_uri, cfg.redirect_policy) {
        return Err(Box::new(token_error_response(
            "invalid_request",
            "redirect_uri violates server redirect policy",
            StatusCode::BAD_REQUEST,
        )));
    }
    Ok(())
}

fn validate_auth_code_record(
    record: &AuthCodeRecord,
    client_id: &str,
    redirect_uri: &str,
) -> Result<(), BoxResponse> {
    if unix_now_secs() > record.expires_at_unix {
        return Err(Box::new(token_error_response(
            "invalid_grant",
            "authorization code expired",
            StatusCode::BAD_REQUEST,
        )));
    }
    if record.client_id != client_id || record.redirect_uri != redirect_uri {
        return Err(Box::new(token_error_response(
            "invalid_grant",
            "authorization code does not match client_id/redirect_uri",
            StatusCode::BAD_REQUEST,
        )));
    }
    Ok(())
}

fn validate_pkce(
    record: &AuthCodeRecord,
    code_verifier: Option<String>,
) -> Result<(), BoxResponse> {
    let Some(challenge) = record.code_challenge.as_ref() else {
        return Ok(());
    };
    let verifier = code_verifier.ok_or_else(|| {
        Box::new(token_error_response(
            "invalid_request",
            "code_verifier is required for PKCE",
            StatusCode::BAD_REQUEST,
        ))
    })?;
    let method = record.code_challenge_method.as_deref().unwrap_or("S256");
    if method != "S256" {
        return Err(Box::new(token_error_response(
            "invalid_request",
            "code_challenge_method must be S256",
            StatusCode::BAD_REQUEST,
        )));
    }
    let digest = Sha256::digest(verifier.as_bytes());
    let computed = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    if computed != *challenge {
        return Err(Box::new(token_error_response(
            "invalid_grant",
            "invalid code_verifier",
            StatusCode::BAD_REQUEST,
        )));
    }
    Ok(())
}

async fn issue_oauth_tokens(
    state: &GoogleOAuthState,
    client_id: String,
    scope: String,
) -> Result<OAuthTokenResponse, Response> {
    let access_token = format!("atk_{}", Uuid::new_v4());
    let refresh_token = format!("rtk_{}", Uuid::new_v4());
    let expires_in = 3600_u64;
    let access_record = AccessTokenRecord {
        scope: scope.clone(),
        expires_at_unix: unix_now_secs() + expires_in,
    };
    state
        .put_access_token(&access_token, &access_record, expires_in)
        .await?;

    let refresh_record = RefreshTokenRecord {
        client_id,
        scope,
        expires_at_unix: unix_now_secs() + OAUTH_REFRESH_TTL_SECS,
    };
    state
        .put_refresh_token(&refresh_token, &refresh_record, OAUTH_REFRESH_TTL_SECS)
        .await?;

    Ok(OAuthTokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in,
        refresh_token: Some(refresh_token),
        scope: access_record.scope,
    })
}

fn parse_refresh_grant_input(form: &TokenRequest) -> Result<RefreshGrantInput, BoxResponse> {
    let client_id = required_form_field(form.client_id.clone(), "client_id")?;
    let refresh_token = required_form_field(form.refresh_token.clone(), "refresh_token")?;
    Ok(RefreshGrantInput {
        client_id,
        refresh_token,
    })
}

fn validate_refresh_token_record(
    record: &RefreshTokenRecord,
    client_id: &str,
) -> Result<(), BoxResponse> {
    if record.client_id != client_id {
        return Err(Box::new(token_error_response(
            "invalid_grant",
            "refresh_token does not belong to this client",
            StatusCode::BAD_REQUEST,
        )));
    }
    if unix_now_secs() > record.expires_at_unix {
        return Err(Box::new(token_error_response(
            "invalid_grant",
            "refresh_token expired",
            StatusCode::BAD_REQUEST,
        )));
    }
    Ok(())
}

async fn rotate_refresh_token(
    state: &GoogleOAuthState,
    client_id: String,
    scope: String,
    refresh_to_revoke: &str,
) -> Result<OAuthTokenResponse, Response> {
    let access_token = format!("atk_{}", Uuid::new_v4());
    let new_refresh_token = format!("rtk_{}", Uuid::new_v4());
    let expires_in = 3600_u64;
    let access_record = AccessTokenRecord {
        scope: scope.clone(),
        expires_at_unix: unix_now_secs() + expires_in,
    };
    state
        .put_access_token(&access_token, &access_record, expires_in)
        .await?;

    let rotated_refresh = RefreshTokenRecord {
        client_id,
        scope,
        expires_at_unix: unix_now_secs() + OAUTH_REFRESH_TTL_SECS,
    };
    state
        .put_refresh_token(&new_refresh_token, &rotated_refresh, OAUTH_REFRESH_TTL_SECS)
        .await?;

    state.delete_refresh_token(refresh_to_revoke).await;
    Ok(OAuthTokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in,
        refresh_token: Some(new_refresh_token),
        scope: access_record.scope,
    })
}

async fn handle_auth_code_grant(
    state: &GoogleOAuthState,
    identity: &str,
    form: TokenRequest,
) -> Response {
    let input = match parse_auth_code_grant_input(&form) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    if let Err(resp) = validate_redirect_policy(state, &input.redirect_uri) {
        return *resp;
    }
    let record = match state.consume_auth_code(&input.code).await {
        Some(v) => v,
        None => {
            return token_error_response(
                "invalid_grant",
                "invalid or expired authorization code",
                StatusCode::BAD_REQUEST,
            );
        }
    };
    if let Err(resp) = validate_auth_code_record(&record, &input.client_id, &input.redirect_uri) {
        return *resp;
    }
    if let Err(resp) = validate_pkce(&record, form.code_verifier.clone()) {
        return *resp;
    }
    let token_response = match issue_oauth_tokens(state, input.client_id, record.scope).await {
        Ok(resp) => resp,
        Err(resp) => return resp,
    };
    info!(
        target: "axon.mcp.oauth",
        identity,
        "token exchange succeeded (authorization_code)"
    );
    (StatusCode::OK, Json(token_response)).into_response()
}

async fn handle_refresh_token_grant(
    state: &GoogleOAuthState,
    identity: &str,
    form: TokenRequest,
) -> Response {
    let input = match parse_refresh_grant_input(&form) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    let refresh_record = match state.get_refresh_token(&input.refresh_token).await {
        Some(v) => v,
        None => {
            return token_error_response(
                "invalid_grant",
                "invalid refresh_token",
                StatusCode::BAD_REQUEST,
            );
        }
    };
    if let Err(resp) = validate_refresh_token_record(&refresh_record, &input.client_id) {
        return *resp;
    }
    let token_response = match rotate_refresh_token(
        state,
        input.client_id,
        refresh_record.scope,
        &input.refresh_token,
    )
    .await
    {
        Ok(resp) => resp,
        Err(resp) => return resp,
    };
    info!(
        target: "axon.mcp.oauth",
        identity,
        "token exchange succeeded (refresh_token)"
    );
    (StatusCode::OK, Json(token_response)).into_response()
}

pub(crate) async fn oauth_token(
    State(state): State<GoogleOAuthState>,
    headers: axum::http::HeaderMap,
    Form(form): Form<TokenRequest>,
) -> Response {
    let identity = request_identity_from_headers(&headers);
    if let Err(resp) = state
        .check_rate_limit(&format!("token:{identity}"), 120, 60)
        .await
    {
        return resp;
    }
    let grant = form.grant_type.clone();
    match grant.as_str() {
        "authorization_code" => handle_auth_code_grant(&state, &identity, form).await,
        "refresh_token" => handle_refresh_token_grant(&state, &identity, form).await,
        _ => token_error_response(
            "unsupported_grant_type",
            "supported grant_type values are authorization_code and refresh_token",
            StatusCode::BAD_REQUEST,
        ),
    }
}

pub(crate) async fn require_google_auth(
    State(state): State<GoogleOAuthState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    if !req.uri().path().starts_with("/mcp") {
        return next.run(req).await;
    }

    if let Some(token) = bearer_token_from_headers(req.headers()) {
        if let Some(expected_api_key) = state.inner.mcp_api_key.as_ref()
            && constant_time_eq(token.as_bytes(), expected_api_key.as_bytes())
        {
            return next.run(req).await;
        }

        if state.configured() {
            let record = state.get_access_token(&token).await;

            if let Some(record) = record
                && unix_now_secs() <= record.expires_at_unix
            {
                let needed_scopes = required_scopes(&state);
                let has_all_scopes = needed_scopes
                    .iter()
                    .all(|scope| record.scope.split_whitespace().any(|s| s == scope));
                if has_all_scopes {
                    return next.run(req).await;
                }
                return unauthorized_response(
                    &state,
                    serde_json::json!({
                        "error": "insufficient_scope",
                        "required_scopes": needed_scopes,
                    }),
                );
            }
        }

        return unauthorized_response(
            &state,
            serde_json::json!({
                "error": "invalid_token"
            }),
        );
    }

    if !state.configured() && !state.api_key_configured() {
        return unauthorized_response(
            &state,
            serde_json::json!({
                "error": "authorization_unavailable",
                "error_description": "authorization is not configured — see server documentation"
            }),
        );
    }

    unauthorized_response(
        &state,
        serde_json::json!({
            "error": "authorization_required"
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use serde_json::Value;

    #[test]
    fn required_form_field_missing_returns_bad_request_contract() {
        let resp = *required_form_field(None, "client_id").expect_err("missing field should error");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn parse_auth_code_grant_input_invalid_redirect_preserves_error_shape() {
        let req = TokenRequest {
            grant_type: "authorization_code".to_string(),
            code: Some("code".to_string()),
            redirect_uri: Some("not-a-uri".to_string()),
            client_id: Some("client-1".to_string()),
            code_verifier: None,
            refresh_token: None,
        };

        let resp = match parse_auth_code_grant_input(&req) {
            Ok(_) => panic!("invalid redirect uri should fail parsing"),
            Err(resp) => *resp,
        };
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let bytes = to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body should be readable");
        let parsed: Value = serde_json::from_slice(&bytes).expect("body should be valid json");
        assert_eq!(parsed["error"], "invalid_request");
        assert_eq!(parsed["error_description"], "redirect_uri is invalid");
    }
}
