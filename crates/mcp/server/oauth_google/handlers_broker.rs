use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use reqwest::Url;
use tracing::{info, warn};
use uuid::Uuid;

use super::helpers::{
    append_query_pairs, bearer_token_from_headers, constant_time_eq, extract_cookie_value,
    is_allowed_redirect_uri, normalize_loopback_redirect_uri, request_identity,
    request_identity_from_headers, session_cookie_name, unix_now_secs,
};
use super::types::{
    AuthCodeRecord, AuthorizeErrorResponse, AuthorizeParams, DynamicClientRegistrationRequest,
    DynamicClientRegistrationResponse, GoogleOAuthState, RegisteredClient,
};

#[allow(clippy::result_large_err)]
fn validate_registration_auth_token(
    expected_token: Option<&String>,
    headers: &axum::http::HeaderMap,
    identity: &str,
) -> Result<(), Response> {
    if let Some(expected) = expected_token {
        let provided = bearer_token_from_headers(headers);
        let token_valid = provided
            .as_deref()
            .map(|p| constant_time_eq(p.as_bytes(), expected.as_bytes()))
            .unwrap_or(false);
        if !token_valid {
            warn!(
                target: "axon.mcp.oauth",
                identity,
                "dynamic client registration rejected: missing/invalid token"
            );
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "invalid_client",
                    "error_description": "dynamic registration requires bearer token"
                })),
            )
                .into_response());
        }
    }
    Ok(())
}

#[allow(clippy::result_large_err)]
fn extract_valid_redirect_uris(
    payload: DynamicClientRegistrationRequest,
    redirect_policy: super::types::RedirectPolicy,
) -> Result<Vec<String>, Response> {
    let redirect_uris = payload
        .redirect_uris
        .filter(|uris| !uris.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_redirect_uri",
                    "error_description": "redirect_uris is required"
                })),
            )
                .into_response()
        })?
        .into_iter()
        .filter_map(|uri| normalize_loopback_redirect_uri(&uri))
        .filter(|uri| is_allowed_redirect_uri(uri, redirect_policy))
        .collect::<Vec<_>>();
    if redirect_uris.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_redirect_uri",
                "error_description": "redirect_uris must include at least one valid URI"
            })),
        )
            .into_response());
    }
    Ok(redirect_uris)
}

pub(crate) async fn oauth_register_client(
    State(state): State<GoogleOAuthState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<DynamicClientRegistrationRequest>,
) -> Result<Json<DynamicClientRegistrationResponse>, Response> {
    let cfg = state.config()?;
    let identity = request_identity_from_headers(&headers);
    state
        .check_rate_limit(&format!("register:{identity}"), 20, 60)
        .await?;

    validate_registration_auth_token(cfg.dcr_token.as_ref(), &headers, &identity)?;

    if let Some(method) = payload.token_endpoint_auth_method.as_deref()
        && method != "none"
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_client_metadata",
                "error_description": "only token_endpoint_auth_method=none is supported"
            })),
        )
            .into_response());
    }

    let redirect_uris = extract_valid_redirect_uris(payload, cfg.redirect_policy)?;

    let client_id = format!("axon-{}", Uuid::new_v4());
    let client = RegisteredClient {
        redirect_uris: redirect_uris.clone(),
    };
    state.put_client(&client_id, &client).await?;
    info!(
        target: "axon.mcp.oauth",
        identity,
        client_id,
        redirect_count = client.redirect_uris.len(),
        "dynamic client registered"
    );

    Ok(Json(DynamicClientRegistrationResponse {
        client_id,
        client_id_issued_at: unix_now_secs(),
        redirect_uris,
        token_endpoint_auth_method: "none".to_string(),
        grant_types: vec![
            "authorization_code".to_string(),
            "refresh_token".to_string(),
        ],
        response_types: vec!["code".to_string()],
    }))
}

// RFC 6749 §4.1.2.1: validate client_id and redirect_uri BEFORE any error
// redirects. Error responses must NOT be sent to an unvalidated redirect_uri.
#[allow(clippy::result_large_err)]
fn validate_authorize_redirect_uri(
    state: &GoogleOAuthState,
    params: &AuthorizeParams,
    registered: &RegisteredClient,
) -> Result<String, Response> {
    let redirect_uri = match params.redirect_uri.clone() {
        Some(uri) => uri,
        None => {
            if registered.redirect_uris.len() == 1 {
                registered.redirect_uris[0].clone()
            } else {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(AuthorizeErrorResponse {
                        error: "invalid_request".to_string(),
                        error_description: "redirect_uri is required".to_string(),
                    }),
                )
                    .into_response());
            }
        }
    };
    let redirect_uri = match normalize_loopback_redirect_uri(&redirect_uri) {
        Some(uri) => uri,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(AuthorizeErrorResponse {
                    error: "invalid_request".to_string(),
                    error_description: "redirect_uri is invalid".to_string(),
                }),
            )
                .into_response());
        }
    };
    let cfg = state.config()?;
    if !is_allowed_redirect_uri(&redirect_uri, cfg.redirect_policy) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(AuthorizeErrorResponse {
                error: "invalid_request".to_string(),
                error_description: "redirect_uri violates server redirect policy".to_string(),
            }),
        )
            .into_response());
    }
    if !registered
        .redirect_uris
        .iter()
        .any(|uri| uri == &redirect_uri)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(AuthorizeErrorResponse {
                error: "invalid_request".to_string(),
                error_description: "redirect_uri is not registered for this client".to_string(),
            }),
        )
            .into_response());
    }
    Ok(redirect_uri)
}

#[allow(clippy::result_large_err)]
fn validate_pkce_params(params: &AuthorizeParams) -> Result<(), Response> {
    if params.code_challenge.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(AuthorizeErrorResponse {
                error: "invalid_request".to_string(),
                error_description: "code_challenge is required".to_string(),
            }),
        )
            .into_response());
    }
    if params.code_challenge_method.as_deref() != Some("S256") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(AuthorizeErrorResponse {
                error: "invalid_request".to_string(),
                error_description: "code_challenge_method must be S256".to_string(),
            }),
        )
            .into_response());
    }
    Ok(())
}

#[allow(clippy::result_large_err)]
fn validate_scope(
    scope_opt: Option<String>,
    allowed_scopes: &[String],
) -> Result<String, Response> {
    let scope = scope_opt.unwrap_or_else(|| "openid email profile".to_string());
    let requested: Vec<&str> = scope.split_whitespace().collect();
    for s in &requested {
        if !allowed_scopes.iter().any(|a| a == s) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(AuthorizeErrorResponse {
                    error: "invalid_scope".to_string(),
                    error_description: format!("scope '{s}' is not allowed"),
                }),
            )
                .into_response());
        }
    }
    Ok(scope)
}

fn redirect_to_login(req: &axum::extract::Request) -> Response {
    let return_to = req.uri().to_string();
    let mut login_url = Url::parse("http://localhost/oauth/google/login").unwrap_or_else(|_| {
        Url::parse("http://localhost/")
            .unwrap_or_else(|_| unreachable!("static URL literal cannot fail to parse"))
    });
    login_url
        .query_pairs_mut()
        .append_pair("return_to", &return_to);
    let redirect = format!(
        "/oauth/google/login?{}",
        login_url.query().unwrap_or_default()
    );
    Redirect::temporary(&redirect).into_response()
}

fn unauthorized_client_response() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(AuthorizeErrorResponse {
            error: "unauthorized_client".to_string(),
            error_description: "unknown client_id".to_string(),
        }),
    )
        .into_response()
}

#[allow(clippy::result_large_err)]
async fn load_authorize_context(
    state: &GoogleOAuthState,
    params: AuthorizeParams,
) -> Result<(String, Option<String>, AuthCodeRecord), Response> {
    let registered = state
        .get_client(&params.client_id)
        .await
        .ok_or_else(unauthorized_client_response)?;
    let redirect_uri = validate_authorize_redirect_uri(state, &params, &registered)?;
    if params.response_type != "code" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(AuthorizeErrorResponse {
                error: "unsupported_response_type".to_string(),
                error_description: "only response_type=code is supported".to_string(),
            }),
        )
            .into_response());
    }
    validate_pkce_params(&params)?;
    let cfg = state.config()?;
    let scope = validate_scope(params.scope, &cfg.scopes)?;
    let state_param = params.state;
    Ok((
        redirect_uri.clone(),
        state_param,
        AuthCodeRecord {
            client_id: params.client_id,
            redirect_uri,
            scope,
            code_challenge: params.code_challenge,
            code_challenge_method: Some("S256".to_string()),
            expires_at_unix: unix_now_secs() + 600,
        },
    ))
}

pub(crate) async fn oauth_authorize(
    State(state): State<GoogleOAuthState>,
    Query(params): Query<AuthorizeParams>,
    req: axum::extract::Request,
) -> Response {
    let identity = request_identity(&req);
    if let Err(resp) = state
        .check_rate_limit(&format!("authorize:{identity}"), 60, 60)
        .await
    {
        return resp;
    }

    let session_id = extract_cookie_value(&req, session_cookie_name(&state));
    let authenticated = match session_id.as_deref() {
        Some(id) => state.is_authenticated(id).await,
        None => false,
    };
    if !authenticated {
        return redirect_to_login(&req);
    }

    let (redirect_uri, state_param, record) = match load_authorize_context(&state, params).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let auth_code = Uuid::new_v4().to_string();
    if let Err(resp) = state.put_auth_code(&auth_code, &record).await {
        return resp;
    }
    info!(
        target: "axon.mcp.oauth",
        identity,
        client_id = record.client_id,
        "authorization code issued"
    );

    let mut query = vec![("code", auth_code)];
    if let Some(state_param) = state_param {
        query.push(("state", state_param));
    }

    match append_query_pairs(&redirect_uri, &query) {
        Ok(url) => Redirect::temporary(&url).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(AuthorizeErrorResponse {
                error: "invalid_request".to_string(),
                error_description: e,
            }),
        )
            .into_response(),
    }
}
