use super::*;

impl SqliteStore {
    pub async fn upsert_browser_session(
        &self,
        session: BrowserSessionRow,
    ) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO browser_sessions (
                    session_id, subject, email, csrf_token, created_at, expires_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(session_id) DO UPDATE SET
                    subject = excluded.subject,
                    email = excluded.email,
                    csrf_token = excluded.csrf_token,
                    created_at = excluded.created_at,
                    expires_at = excluded.expires_at",
                params![
                    session.session_id,
                    session.subject,
                    session.email,
                    session.csrf_token,
                    session.created_at,
                    session.expires_at,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn find_browser_session(
        &self,
        session_id: &str,
    ) -> Result<Option<BrowserSessionRow>, AuthError> {
        let session_id = session_id.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT session_id, subject, email, csrf_token, created_at, expires_at
                 FROM browser_sessions
                 WHERE session_id = ?1
                   AND expires_at > ?2",
                params![session_id, now],
                row_to_browser_session,
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }

    pub async fn revoke_browser_session(&self, session_id: &str) -> Result<(), AuthError> {
        let session_id = session_id.to_string();
        self.with_conn(move |conn| {
            conn.execute(
                "DELETE FROM browser_sessions WHERE session_id = ?1",
                params![session_id],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn execute_test_statement(&self, sql: &str) -> Result<(), AuthError> {
        let sql = sql.to_string();
        self.with_conn(move |conn| conn.execute_batch(&sql).map_err(sqlite_error))
            .await
    }

    pub async fn insert_browser_login_state(
        &self,
        login: BrowserLoginStateRow,
    ) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO browser_login_states (
                    state, return_to, provider_code_verifier, created_at, expires_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    login.state,
                    login.return_to,
                    login.provider_code_verifier,
                    login.created_at,
                    login.expires_at,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn count_pending_oauth_states(&self) -> Result<usize, AuthError> {
        let now = now_unix();
        self.with_conn(move |conn| {
            let authorization_requests: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM authorization_requests WHERE expires_at > ?1",
                    params![now],
                    |row| row.get(0),
                )
                .map_err(sqlite_error)?;
            let browser_login_states: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM browser_login_states WHERE expires_at > ?1",
                    params![now],
                    |row| row.get(0),
                )
                .map_err(sqlite_error)?;
            let native_authorization_results: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM native_authorization_results WHERE expires_at > ?1",
                    params![now],
                    |row| row.get(0),
                )
                .map_err(sqlite_error)?;
            Ok(
                (authorization_requests + browser_login_states + native_authorization_results)
                    as usize,
            )
        })
        .await
    }

    pub async fn take_browser_login_state(
        &self,
        state: &str,
    ) -> Result<Option<BrowserLoginStateRow>, AuthError> {
        let state = state.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.query_row(
                "DELETE FROM browser_login_states
                 WHERE state = ?1
                   AND expires_at > ?2
                 RETURNING state, return_to, provider_code_verifier, created_at, expires_at",
                params![state, now],
                row_to_browser_login_state,
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }

    pub async fn insert_native_authorization_result(
        &self,
        result: NativeAuthorizationResultRow,
    ) -> Result<(), AuthError> {
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO native_authorization_results (state, code, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(state) DO NOTHING",
                params![
                    result.state,
                    result.code,
                    result.created_at,
                    result.expires_at,
                ],
            )
            .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    pub async fn take_native_authorization_result(
        &self,
        state: &str,
    ) -> Result<Option<NativeAuthorizationResultRow>, AuthError> {
        let state = state.to_string();
        let now = now_unix();
        self.with_conn(move |conn| {
            conn.query_row(
                "DELETE FROM native_authorization_results
                 WHERE state = ?1
                   AND expires_at > ?2
                 RETURNING state, code, created_at, expires_at",
                params![state, now],
                row_to_native_authorization_result,
            )
            .optional()
            .map_err(sqlite_error)
        })
        .await
    }
}
