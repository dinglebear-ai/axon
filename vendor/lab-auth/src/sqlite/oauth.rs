use super::*;

impl SqliteStore {
    pub async fn register_client(&self, client: RegisteredClient) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            let redirect_uris = serde_json::to_string(&client.redirect_uris)
                .map_err(|error| AuthError::Storage(format!("serialize redirect_uris: {error}")))?;
            conn.execute(
                "INSERT INTO registered_clients (client_id, redirect_uris, created_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(client_id) DO UPDATE SET
                    redirect_uris = excluded.redirect_uris,
                    created_at = excluded.created_at",
                params![client.client_id, redirect_uris, client.created_at],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn find_client(
        &self,
        client_id: &str,
    ) -> Result<Option<RegisteredClient>, AuthError> {
        let client_id = client_id.to_string();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT client_id, redirect_uris, created_at
                 FROM registered_clients
                 WHERE client_id = ?1",
                params![client_id],
                |row| {
                    let redirect_uris: String = row.get(1)?;
                    let redirect_uris = serde_json::from_str(&redirect_uris).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                    Ok(RegisteredClient {
                        client_id: row.get(0)?,
                        redirect_uris,
                        created_at: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }

    pub async fn insert_authorization_request(
        &self,
        request: AuthorizationRequestRow,
    ) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO authorization_requests (
                    state, client_id, redirect_uri, client_state, scope, provider_code_verifier,
                    code_challenge, code_challenge_method, created_at, expires_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    request.state,
                    request.client_id,
                    request.redirect_uri,
                    request.client_state,
                    request.scope,
                    request.provider_code_verifier,
                    request.code_challenge,
                    request.code_challenge_method,
                    request.created_at,
                    request.expires_at,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn take_authorization_request(
        &self,
        state: &str,
    ) -> Result<AuthorizationRequestRow, AuthError> {
        let state = state.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.query_row(
                "DELETE FROM authorization_requests
                 WHERE state = ?1
                   AND expires_at > ?2
                 RETURNING state, client_id, redirect_uri, client_state, scope, provider_code_verifier,
                           code_challenge, code_challenge_method, created_at, expires_at",
                params![state, now],
                row_to_authorization_request,
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => AuthError::InvalidGrant(
                    "authorization state is missing, expired, or already used".to_string(),
                ),
                other => sqlite_error(other),
            })
        })
        .await
    }

    pub async fn insert_auth_code(&self, code: AuthorizationCodeRow) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO authorization_codes (
                    code, client_id, subject, redirect_uri, scope,
                    code_challenge, code_challenge_method, provider_refresh_token,
                    created_at, expires_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    code.code,
                    code.client_id,
                    code.subject,
                    code.redirect_uri,
                    code.scope,
                    code.code_challenge,
                    code.code_challenge_method,
                    code.provider_refresh_token,
                    code.created_at,
                    code.expires_at,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn redeem_auth_code(&self, code: &str) -> Result<AuthorizationCodeRow, AuthError> {
        let code = code.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.query_row(
                "DELETE FROM authorization_codes
                 WHERE code = ?1
                   AND expires_at > ?2
                 RETURNING code, client_id, subject, redirect_uri, scope,
                           code_challenge, code_challenge_method, provider_refresh_token,
                           created_at, expires_at",
                params![code, now],
                row_to_authorization_code,
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => AuthError::InvalidGrant(
                    "authorization code is missing, expired, or already redeemed".to_string(),
                ),
                other => sqlite_error(other),
            })
        })
        .await
    }

    pub async fn upsert_refresh_token(&self, token: RefreshTokenRow) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO refresh_tokens (
                    refresh_token, client_id, subject, scope,
                    provider_refresh_token, created_at, expires_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(refresh_token) DO UPDATE SET
                    client_id = excluded.client_id,
                    subject = excluded.subject,
                    scope = excluded.scope,
                    provider_refresh_token = excluded.provider_refresh_token, -- gitleaks:allow
                    created_at = excluded.created_at,
                    expires_at = excluded.expires_at",
                params![
                    token.refresh_token,
                    token.client_id,
                    token.subject,
                    token.scope,
                    token.provider_refresh_token,
                    token.created_at,
                    token.expires_at,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn find_refresh_token(
        &self,
        refresh_token: &str,
    ) -> Result<Option<RefreshTokenRow>, AuthError> {
        let refresh_token = refresh_token.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT refresh_token, client_id, subject, scope,
                        provider_refresh_token, created_at, expires_at
                 FROM refresh_tokens
                 WHERE refresh_token = ?1
                   AND expires_at > ?2",
                params![refresh_token, now],
                row_to_refresh_token,
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }
}
