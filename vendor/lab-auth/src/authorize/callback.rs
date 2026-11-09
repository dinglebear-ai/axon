use super::*;

pub async fn callback(
    State(state): State<AuthState>,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, AuthError> {
    let oauth_state_id = fingerprint(&query.state);
    info!(
        oauth_state_id = %oauth_state_id,
        provider = "google",
        "oauth callback received"
    );
    if let Some(login) = state.store.take_browser_login_state(&query.state).await? {
        return complete_browser_login(&state, &query, login, &oauth_state_id).await;
    }

    let request = state
        .store
        .take_authorization_request(&query.state)
        .await
        .map_err(|_| {
            warn!(
                oauth_state_id = %oauth_state_id,
                "oauth callback rejected: authorization state is invalid or expired"
            );
            AuthError::InvalidGrant("authorization state is invalid or expired".to_string())
        })?;
    info!(
        client_id = %request.client_id,
        redirect_uri = %request.redirect_uri,
        oauth_state_id = %oauth_state_id,
        client_state_id = %fingerprint(&request.client_state),
        "oauth callback state redeemed"
    );
    let google = state
        .google
        .exchange_code(&query.code, &request.provider_code_verifier)
        .await?;

    // RFC 6749 §4.1.2.1: errors must redirect to the client's redirect_uri,
    // not surface as a JSON HTTP error. The denial reason is sourced from the
    // AuthError so we only log once (inside check_email_allowlist).
    let allowed = state.resolve_allowed_emails().await?;
    if let Err(denial) =
        check_email_allowlist(google.email.as_deref(), google.email_verified, &allowed)
    {
        let mut redirect_target = url::Url::parse(&request.redirect_uri).map_err(|error| {
            // Unreachable in practice: redirect_uri was validated against the
            // client's registered URIs before being stored.
            AuthError::Config(format!("failed to parse registered redirect_uri: {error}"))
        })?;
        redirect_target
            .query_pairs_mut()
            .append_pair("error", "access_denied")
            .append_pair("error_description", &denial.to_string())
            .append_pair("state", &request.client_state);
        return Ok(Redirect::to(redirect_target.as_str()).into_response());
    }

    let subject_id = fingerprint(&google.subject);
    let granted_scope =
        granted_scope_for_oauth_user(&state.config, google.email.as_deref(), &request.scope);
    info!(
        client_id = %request.client_id,
        oauth_state_id = %oauth_state_id,
        subject_id = %subject_id,
        has_provider_refresh_token = google.refresh_token.is_some(),
        "oauth callback exchanged upstream code successfully"
    );
    let auth_code = random_token(24)?;
    let auth_code_id = fingerprint(&auth_code);
    state
        .store
        .insert_auth_code(AuthorizationCodeRow {
            code: auth_code.clone(),
            client_id: request.client_id,
            subject: google.subject,
            redirect_uri: request.redirect_uri.clone(),
            scope: granted_scope,
            code_challenge: request.code_challenge,
            code_challenge_method: request.code_challenge_method,
            provider_refresh_token: google.refresh_token,
            created_at: now_unix(),
            expires_at: expires_at(
                now_unix(),
                state.config.auth_code_ttl,
                "LAB_AUTH_CODE_TTL_SECS",
            )?,
        })
        .await?;
    info!(
        auth_code_id = %auth_code_id,
        oauth_state_id = %oauth_state_id,
        redirect_uri = %request.redirect_uri,
        "oauth callback issued local authorization code"
    );

    let native_callback_endpoint = crate::metadata::native_callback_endpoint(&state);
    if request.redirect_uri == native_callback_endpoint {
        return complete_native_login(
            &state,
            request.client_state,
            auth_code,
            &auth_code_id,
            &native_callback_endpoint,
        )
        .await;
    }

    let redirect_uri = reqwest::Url::parse(&request.redirect_uri).map_err(|error| {
        AuthError::Storage(format!(
            "registered redirect_uri is not a valid URL: {error}"
        ))
    })?;
    let mut redirect_uri = redirect_uri;
    redirect_uri
        .query_pairs_mut()
        .append_pair("code", &auth_code)
        .append_pair("state", &request.client_state);
    // The completed URI contains the authorization code and client
    // state. Keep only the one-way code fingerprint in diagnostic output.
    debug!(
        auth_code_id = %auth_code_id,
        "oauth callback redirecting client back to registered callback"
    );

    Ok(Redirect::to(redirect_uri.as_str()).into_response())
}

async fn complete_browser_login(
    state: &AuthState,
    query: &CallbackQuery,
    login: BrowserLoginStateRow,
    oauth_state_id: &str,
) -> Result<Response, AuthError> {
    let google = state
        .google
        .exchange_code(&query.code, &login.provider_code_verifier)
        .await?;
    let allowed = state.resolve_allowed_emails().await?;
    check_email_allowlist(google.email.as_deref(), google.email_verified, &allowed)?;
    let session = create_browser_session(&state, google.subject, google.email).await?;
    let mut response = Redirect::to(&login.return_to).into_response();
    append_set_cookie(
        &mut response,
        &build_browser_session_cookie(&state, &session.session_id),
    );
    info!(
        oauth_state_id = %oauth_state_id,
        return_to = %login.return_to,
        subject_id = %fingerprint(&session.subject),
        "browser login callback issued session cookie"
    );
    Ok(response)
}

async fn complete_native_login(
    state: &AuthState,
    client_state: String,
    auth_code: String,
    auth_code_id: &str,
    native_callback_endpoint: &str,
) -> Result<Response, AuthError> {
    let now = now_unix();
    state
        .store
        .insert_native_authorization_result(NativeAuthorizationResultRow {
            state: client_state,
            code: auth_code,
            created_at: now,
            expires_at: expires_at(now, state.config.auth_code_ttl, "LAB_AUTH_CODE_TTL_SECS")?,
        })
        .await?;
    let mut response = axum::response::Html(NATIVE_SUCCESS_PAGE).into_response();
    no_store(&mut response);
    debug!(
        auth_code_id = %auth_code_id,
        native_callback_endpoint = %native_callback_endpoint,
        "oauth callback stored native authorization code for polling"
    );
    Ok(response)
}
