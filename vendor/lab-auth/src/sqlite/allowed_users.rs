use super::*;

impl SqliteStore {
    /// Add an email address to the allowlist.
    ///
    /// `email` is normalised to lowercase before storage. Returns
    /// `AuthError::Validation` if the email is already present.
    pub async fn add_allowed_user(
        &self,
        email: &str,
        added_by: &str,
        created_at: i64,
    ) -> Result<(), AuthError> {
        let email = email.to_lowercase();
        let fp = fingerprint(&email);
        let added_by = added_by.to_string();
        self.with_conn(move |conn| {
            let changed = conn
                .execute(
                    "INSERT INTO allowed_users (email, added_by, created_at)
                     VALUES (?1, ?2, ?3)",
                    params![email, added_by, created_at],
                )
                .map_err(|error| match error {
                    rusqlite::Error::SqliteFailure(ref e, _)
                        if e.code == rusqlite::ErrorCode::ConstraintViolation =>
                    {
                        AuthError::Validation(format!(
                            "email fingerprint {fp} is already in the allowlist"
                        ))
                    }
                    other => sqlite_error(other),
                })?;
            debug_assert_eq!(changed, 1);
            Ok(())
        })
        .await
    }

    /// Remove an email address from the allowlist.
    ///
    /// Idempotent: returns `Ok(())` even if the email was not present.
    pub async fn remove_allowed_user(&self, email: &str) -> Result<(), AuthError> {
        let email = email.to_lowercase();
        self.with_conn(move |conn| {
            conn.execute("DELETE FROM allowed_users WHERE email = ?1", params![email])
                .map_err(sqlite_error)?;
            Ok(())
        })
        .await
    }

    /// Return all allowlist rows ordered by `created_at ASC`.
    pub async fn list_allowed_users(&self) -> Result<Vec<AllowedUserRow>, AuthError> {
        self.with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT email, added_by, created_at
                     FROM allowed_users
                     ORDER BY created_at ASC",
                )
                .map_err(sqlite_error)?;
            let rows = stmt
                .query_map([], row_to_allowed_user)
                .map_err(sqlite_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(sqlite_error)?;
            Ok(rows)
        })
        .await
    }
}
