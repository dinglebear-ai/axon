use std::path::PathBuf;

use crate::types::{
    AllowedUserRow, AuthorizationCodeRow, BrowserSessionRow, NativeAuthorizationResultRow,
    RefreshTokenRow, UpstreamOauthCredentialRow, UpstreamOauthStateRow,
};

use crate::util::now_unix;

use super::{SQLITE_POOL_SIZE, SqliteStore};

#[tokio::test]
async fn sqlite_store_enables_wal_and_busy_timeout() {
    let store = temp_store().await;
    assert_eq!(pragma(&store, "journal_mode").await, "wal");
    assert!(pragma_ms(&store, "busy_timeout").await >= 5_000);
}

#[tokio::test]
async fn sqlite_store_opens_multiple_connections() {
    let store = temp_store().await;
    assert_eq!(store.connection_count(), SQLITE_POOL_SIZE);
}

#[tokio::test]
async fn sqlite_store_redeems_auth_code_only_once_under_race() {
    let store = temp_store().await;
    store.insert_auth_code(sample_code()).await.unwrap();
    let (a, b) = tokio::join!(
        store.redeem_auth_code("code-123"),
        store.redeem_auth_code("code-123"),
    );
    assert!(a.is_ok() ^ b.is_ok(), "a={a:?} b={b:?}");
}

#[tokio::test]
async fn sqlite_store_takes_native_authorization_result_once() {
    let store = temp_store().await;
    store
        .insert_native_authorization_result(NativeAuthorizationResultRow {
            state: "state-123".to_string(),
            code: "code-123".to_string(),
            created_at: now_unix(),
            expires_at: now_unix() + 300,
        })
        .await
        .unwrap();

    let first = store
        .take_native_authorization_result("state-123")
        .await
        .unwrap();
    assert_eq!(first.map(|row| row.code), Some("code-123".to_string()));
    assert!(
        store
            .take_native_authorization_result("state-123")
            .await
            .unwrap()
            .is_none()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn sqlite_store_refuses_world_readable_database_file() {
    use std::os::unix::fs::PermissionsExt;

    let path = temp_db_path();
    std::fs::write(&path, []).unwrap();
    std::fs::set_permissions(&path, PermissionsExt::from_mode(0o644)).unwrap();
    let err = SqliteStore::open(path).await.unwrap_err();
    assert!(err.to_string().contains("permissions"));
}

#[tokio::test]
async fn sqlite_store_rejects_expired_authorization_code() {
    let store = temp_store().await;
    let mut code = sample_code();
    code.expires_at = now_unix() - 1;
    store.insert_auth_code(code).await.unwrap();
    let err = store.redeem_auth_code("code-123").await.unwrap_err();
    assert!(err.to_string().contains("expired"));
}

#[tokio::test]
async fn sqlite_store_ignores_expired_refresh_token() {
    let store = temp_store().await;
    store
        .upsert_refresh_token(RefreshTokenRow {
            refresh_token: "refresh-token".to_string(),
            client_id: "client".to_string(),
            subject: "google-user".to_string(),
            scope: "lab".to_string(),
            provider_refresh_token: Some("provider-refresh".to_string()),
            created_at: now_unix() - 300,
            expires_at: now_unix() - 1,
        })
        .await
        .unwrap();
    assert!(
        store
            .find_refresh_token("refresh-token")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn sqlite_store_cleanup_expired_removes_stale_rows() {
    let store = temp_store().await;
    let now = now_unix();

    // Insert an expired auth code.
    let mut code = sample_code();
    code.expires_at = now - 10;
    store.insert_auth_code(code).await.unwrap();

    // Insert an expired refresh token.
    store
        .upsert_refresh_token(RefreshTokenRow {
            refresh_token: "expired-refresh".to_string(),
            client_id: "client".to_string(),
            subject: "google-user".to_string(),
            scope: "lab".to_string(),
            provider_refresh_token: None,
            created_at: now - 600,
            expires_at: now - 10,
        })
        .await
        .unwrap();

    // Insert an expired authorization request.
    use crate::types::AuthorizationRequestRow;
    store
        .insert_authorization_request(AuthorizationRequestRow {
            state: "expired-state".to_string(),
            client_id: "client".to_string(),
            redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
            client_state: "cs".to_string(),
            scope: "lab".to_string(),
            provider_code_verifier: "verifier".to_string(),
            code_challenge: "challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            created_at: now - 600,
            expires_at: now - 10,
        })
        .await
        .unwrap();

    // Insert a valid (non-expired) refresh token.
    store
        .upsert_refresh_token(RefreshTokenRow {
            refresh_token: "valid-refresh".to_string(),
            client_id: "client".to_string(),
            subject: "google-user".to_string(),
            scope: "lab".to_string(),
            provider_refresh_token: None,
            created_at: now,
            expires_at: now + 3600,
        })
        .await
        .unwrap();

    let deleted = store.cleanup_expired().await.unwrap();
    assert_eq!(deleted, 3, "should delete exactly 3 expired rows");

    // The valid refresh token should still exist.
    assert!(
        store
            .find_refresh_token("valid-refresh")
            .await
            .unwrap()
            .is_some()
    );
}

async fn temp_store() -> SqliteStore {
    SqliteStore::open(temp_db_path()).await.unwrap()
}

async fn pragma(store: &SqliteStore, name: &str) -> String {
    store.pragma(name).await.unwrap()
}

async fn pragma_ms(store: &SqliteStore, name: &str) -> u64 {
    pragma(store, name).await.parse().unwrap()
}

fn temp_db_path() -> PathBuf {
    tempfile::tempdir().unwrap().keep().join("auth.db")
}

fn sample_code() -> AuthorizationCodeRow {
    let now = now_unix();
    AuthorizationCodeRow {
        code: "code-123".to_string(),
        client_id: "client".to_string(),
        subject: "google-user".to_string(),
        redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
        scope: "lab".to_string(),
        code_challenge: "challenge".to_string(),
        code_challenge_method: "S256".to_string(),
        provider_refresh_token: Some("provider-refresh".to_string()),
        created_at: now,
        expires_at: now + 300,
    }
}

#[tokio::test]
async fn browser_session_round_trip_succeeds() {
    let store = temp_store().await;
    let row = BrowserSessionRow {
        session_id: "sess_123".into(),
        subject: "user_1".into(),
        email: Some("jmagar@example.com".into()),
        csrf_token: "csrf_123".into(),
        created_at: 1,
        expires_at: now_unix() + 9_999,
    };

    store.upsert_browser_session(row.clone()).await.unwrap();
    let fetched = store
        .find_browser_session("sess_123")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(fetched.session_id, row.session_id);
    assert_eq!(fetched.subject, row.subject);
    assert_eq!(fetched.csrf_token, row.csrf_token);
}

#[tokio::test]
async fn revoking_browser_session_removes_it() {
    let store = temp_store().await;
    let row = BrowserSessionRow {
        session_id: "sess_123".into(),
        subject: "user_1".into(),
        email: None,
        csrf_token: "csrf_123".into(),
        created_at: 1,
        expires_at: now_unix() + 9_999,
    };

    store.upsert_browser_session(row).await.unwrap();
    store.revoke_browser_session("sess_123").await.unwrap();

    assert!(
        store
            .find_browser_session("sess_123")
            .await
            .unwrap()
            .is_none()
    );
}

// ── allowed_users tests ─────────────────────────────────────────────────

#[tokio::test]
async fn allowed_users_add_and_list() {
    let store = temp_store().await;
    store
        .add_allowed_user("alice@example.com", "admin", now_unix())
        .await
        .unwrap();
    let rows = store.list_allowed_users().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].email, "alice@example.com");
    assert_eq!(rows[0].added_by, "admin");
}

#[tokio::test]
async fn allowed_users_duplicate_returns_validation_error() {
    let store = temp_store().await;
    let now = now_unix();
    store
        .add_allowed_user("bob@example.com", "admin", now)
        .await
        .unwrap();
    let err = store
        .add_allowed_user("bob@example.com", "admin2", now)
        .await
        .unwrap_err();
    assert!(
        matches!(err, crate::error::AuthError::Validation(_)),
        "expected Validation, got {err:?}"
    );
}

#[tokio::test]
async fn allowed_users_input_is_lowercased() {
    let store = temp_store().await;
    store
        .add_allowed_user("Alice@Example.COM", "admin", now_unix())
        .await
        .unwrap();
    let rows = store.list_allowed_users().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].email, "alice@example.com");
}

#[tokio::test]
async fn allowed_users_remove_nonexistent_is_idempotent() {
    let store = temp_store().await;
    // Must not error even when no row exists.
    store
        .remove_allowed_user("nobody@example.com")
        .await
        .unwrap();
}

#[tokio::test]
async fn allowed_users_list_ordered_by_created_at_asc() {
    let store = temp_store().await;
    let base = now_unix();
    store
        .add_allowed_user("third@example.com", "admin", base + 2)
        .await
        .unwrap();
    store
        .add_allowed_user("first@example.com", "admin", base)
        .await
        .unwrap();
    store
        .add_allowed_user("second@example.com", "admin", base + 1)
        .await
        .unwrap();
    let rows = store.list_allowed_users().await.unwrap();
    let emails: Vec<&str> = rows.iter().map(|r| r.email.as_str()).collect();
    assert_eq!(
        emails,
        vec![
            "first@example.com",
            "second@example.com",
            "third@example.com"
        ]
    );
}

#[tokio::test]
async fn allowed_users_schema_bootstrap_is_idempotent() {
    // Open the same file twice; second open must not error.
    let path = temp_db_path();
    let _store1 = SqliteStore::open(path.clone()).await.unwrap();
    let _store2 = SqliteStore::open(path).await.unwrap();
}

// Ensure AllowedUserRow is importable as the right type in tests.
#[allow(dead_code)]
fn _assert_allowed_user_row_type() -> AllowedUserRow {
    AllowedUserRow {
        email: String::new(),
        added_by: String::new(),
        created_at: 0,
    }
}

#[path = "upstream_tests.rs"]
mod upstream;
