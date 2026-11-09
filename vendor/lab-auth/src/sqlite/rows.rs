use super::*;

pub(super) fn row_to_allowed_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<AllowedUserRow> {
    Ok(AllowedUserRow {
        email: row.get(0)?,
        added_by: row.get(1)?,
        created_at: row.get(2)?,
    })
}

pub(super) fn row_to_authorization_request(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AuthorizationRequestRow> {
    Ok(AuthorizationRequestRow {
        state: row.get(0)?,
        client_id: row.get(1)?,
        redirect_uri: row.get(2)?,
        client_state: row.get(3)?,
        scope: row.get(4)?,
        provider_code_verifier: row.get(5)?,
        code_challenge: row.get(6)?,
        code_challenge_method: row.get(7)?,
        created_at: row.get(8)?,
        expires_at: row.get(9)?,
    })
}

pub(super) fn row_to_authorization_code(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AuthorizationCodeRow> {
    Ok(AuthorizationCodeRow {
        code: row.get(0)?,
        client_id: row.get(1)?,
        subject: row.get(2)?,
        redirect_uri: row.get(3)?,
        scope: row.get(4)?,
        code_challenge: row.get(5)?,
        code_challenge_method: row.get(6)?,
        provider_refresh_token: row.get(7)?,
        created_at: row.get(8)?,
        expires_at: row.get(9)?,
    })
}

pub(super) fn row_to_refresh_token(row: &rusqlite::Row<'_>) -> rusqlite::Result<RefreshTokenRow> {
    Ok(RefreshTokenRow {
        refresh_token: row.get(0)?,
        client_id: row.get(1)?,
        subject: row.get(2)?,
        scope: row.get(3)?,
        provider_refresh_token: row.get(4)?,
        created_at: row.get(5)?,
        expires_at: row.get(6)?,
    })
}

pub(super) fn row_to_browser_session(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<BrowserSessionRow> {
    Ok(BrowserSessionRow {
        session_id: row.get(0)?,
        subject: row.get(1)?,
        email: row.get(2)?,
        csrf_token: row.get(3)?,
        created_at: row.get(4)?,
        expires_at: row.get(5)?,
    })
}

pub(super) fn row_to_browser_login_state(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<BrowserLoginStateRow> {
    Ok(BrowserLoginStateRow {
        state: row.get(0)?,
        return_to: row.get(1)?,
        provider_code_verifier: row.get(2)?,
        created_at: row.get(3)?,
        expires_at: row.get(4)?,
    })
}

pub(super) fn row_to_native_authorization_result(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<NativeAuthorizationResultRow> {
    Ok(NativeAuthorizationResultRow {
        state: row.get(0)?,
        code: row.get(1)?,
        created_at: row.get(2)?,
        expires_at: row.get(3)?,
    })
}

pub(super) fn row_to_upstream_oauth_credentials(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<UpstreamOauthCredentialRow> {
    let refresh_token_present: i64 = row.get(8)?;
    Ok(UpstreamOauthCredentialRow {
        upstream_name: row.get(0)?,
        subject: row.get(1)?,
        client_id: row.get(2)?,
        granted_scopes_json: row.get(3)?,
        token_blob: row.get(4)?,
        token_blob_nonce: row.get(5)?,
        token_received_at: row.get(6)?,
        access_token_expires_at: row.get(7)?,
        refresh_token_present: refresh_token_present != 0,
    })
}

pub(super) fn row_to_upstream_oauth_state(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<UpstreamOauthStateRow> {
    Ok(UpstreamOauthStateRow {
        upstream_name: row.get(0)?,
        subject: row.get(1)?,
        csrf_token: row.get(2)?,
        pkce_verifier: row.get(3)?,
        created_at: row.get(4)?,
        expires_at: row.get(5)?,
    })
}
