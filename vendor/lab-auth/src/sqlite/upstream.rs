use super::*;

impl SqliteStore {
    pub async fn upsert_upstream_oauth_credentials(
        &self,
        row: UpstreamOauthCredentialRow,
    ) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO upstream_oauth_credentials (
                    upstream_name, subject, client_id, granted_scopes_json,
                    token_blob, token_blob_nonce, token_received_at,
                    access_token_expires_at, refresh_token_present
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(upstream_name, subject) DO UPDATE SET
                    client_id = excluded.client_id,
                    granted_scopes_json = excluded.granted_scopes_json,
                    token_blob = excluded.token_blob,
                    token_blob_nonce = excluded.token_blob_nonce,
                    token_received_at = excluded.token_received_at,
                    access_token_expires_at = excluded.access_token_expires_at,
                    refresh_token_present = excluded.refresh_token_present",
                params![
                    row.upstream_name,
                    row.subject,
                    row.client_id,
                    row.granted_scopes_json,
                    row.token_blob,
                    row.token_blob_nonce,
                    row.token_received_at,
                    row.access_token_expires_at,
                    i64::from(row.refresh_token_present),
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn find_upstream_oauth_credentials(
        &self,
        upstream_name: &str,
        subject: &str,
    ) -> Result<Option<UpstreamOauthCredentialRow>, AuthError> {
        let upstream_name = upstream_name.to_string();
        let subject = subject.to_string();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT upstream_name, subject, client_id, granted_scopes_json,
                        token_blob, token_blob_nonce, token_received_at,
                        access_token_expires_at, refresh_token_present
                 FROM upstream_oauth_credentials
                 WHERE upstream_name = ?1 AND subject = ?2",
                params![upstream_name, subject],
                row_to_upstream_oauth_credentials,
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }

    pub async fn delete_upstream_oauth_credentials(
        &self,
        upstream_name: &str,
        subject: &str,
    ) -> Result<(), AuthError> {
        let upstream_name = upstream_name.to_string();
        let subject = subject.to_string();
        self.with_conn(move |conn| {
            conn.execute(
                "DELETE FROM upstream_oauth_credentials
                 WHERE upstream_name = ?1 AND subject = ?2",
                params![upstream_name, subject],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn save_upstream_oauth_state(
        &self,
        row: UpstreamOauthStateRow,
    ) -> Result<(), AuthError> {
        if row.expires_at <= row.created_at
            || row.expires_at - row.created_at > UPSTREAM_OAUTH_STATE_MAX_TTL_SECS
        {
            return Err(AuthError::InvalidGrant(
                "state TTL exceeds 600s".to_string(),
            ));
        }
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO upstream_oauth_state (
                    upstream_name, subject, csrf_token, pkce_verifier, created_at, expires_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    row.upstream_name,
                    row.subject,
                    row.csrf_token,
                    row.pkce_verifier,
                    row.created_at,
                    row.expires_at,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn find_upstream_oauth_state_subject(
        &self,
        upstream_name: &str,
        csrf_token: &str,
        now: i64,
    ) -> Result<Option<String>, AuthError> {
        let upstream_name = upstream_name.to_string();
        let csrf_token = csrf_token.to_string();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT subject
                 FROM upstream_oauth_state
                 WHERE upstream_name = ?1
                   AND csrf_token = ?2
                   AND expires_at > ?3",
                params![upstream_name, csrf_token, now],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }

    /// Look up `(upstream_name, subject)` by `csrf_token` alone.
    ///
    /// Used by the OAuth callback handler to recover the upstream identity from
    /// the state parameter without requiring the caller to know it upfront.
    pub async fn find_upstream_oauth_state_owner(
        &self,
        csrf_token: &str,
        now: i64,
    ) -> Result<Option<(String, String)>, AuthError> {
        let csrf_token = csrf_token.to_string();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT upstream_name, subject
                 FROM upstream_oauth_state
                 WHERE csrf_token = ?1
                   AND expires_at > ?2",
                params![csrf_token, now],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }

    /// Delete a pending OAuth state token by CSRF token to foreclose replay attacks after exchange failure.
    pub async fn delete_upstream_oauth_state_by_csrf(
        &self,
        csrf_token: &str,
        now: i64,
    ) -> Result<(), AuthError> {
        let csrf_token = csrf_token.to_string();
        self.with_conn(move |conn| {
            conn.execute(
                "DELETE FROM upstream_oauth_state
                 WHERE csrf_token = ?1
                   AND expires_at > ?2",
                params![csrf_token, now],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    /// Atomic take-once via `DELETE ... RETURNING`.
    pub async fn take_upstream_oauth_state(
        &self,
        upstream_name: &str,
        subject: &str,
        csrf_token: &str,
        now: i64,
    ) -> Result<Option<UpstreamOauthStateRow>, AuthError> {
        let upstream_name = upstream_name.to_string();
        let subject = subject.to_string();
        let csrf_token = csrf_token.to_string();
        self.with_conn(move |conn| {
            conn.query_row(
                "DELETE FROM upstream_oauth_state
                 WHERE upstream_name = ?1
                   AND subject = ?2
                   AND csrf_token = ?3
                   AND expires_at > ?4
                 RETURNING upstream_name, subject, csrf_token, pkce_verifier, created_at, expires_at",
                params![upstream_name, subject, csrf_token, now],
                row_to_upstream_oauth_state,
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }

    pub async fn save_dynamic_client_registration(
        &self,
        upstream_name: &str,
        subject: &str,
        client_id: &str,
    ) -> Result<(), AuthError> {
        let upstream_name = upstream_name.to_string();
        let subject = subject.to_string();
        let client_id = client_id.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO upstream_oauth_dynamic_clients (upstream_name, subject, client_id, created_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(upstream_name, subject) DO UPDATE SET
                    client_id = excluded.client_id,
                    created_at = excluded.created_at",
                params![upstream_name, subject, client_id, now],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn find_dynamic_client_registration(
        &self,
        upstream_name: &str,
        subject: &str,
    ) -> Result<Option<String>, AuthError> {
        let upstream_name = upstream_name.to_string();
        let subject = subject.to_string();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT client_id
                 FROM upstream_oauth_dynamic_clients
                 WHERE upstream_name = ?1 AND subject = ?2",
                params![upstream_name, subject],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }

    pub async fn delete_dynamic_client_registration(
        &self,
        upstream_name: &str,
        subject: &str,
    ) -> Result<(), AuthError> {
        let upstream_name = upstream_name.to_string();
        let subject = subject.to_string();
        self.with_conn(move |conn| {
            conn.execute(
                "DELETE FROM upstream_oauth_dynamic_clients
                 WHERE upstream_name = ?1 AND subject = ?2",
                params![upstream_name, subject],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }
}
