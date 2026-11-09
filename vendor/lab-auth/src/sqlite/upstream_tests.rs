use super::*;

fn sample_upstream_credentials() -> UpstreamOauthCredentialRow {
    let now = now_unix();
    UpstreamOauthCredentialRow {
        upstream_name: "acme".to_string(),
        subject: "alice".to_string(),
        client_id: "client-xyz".to_string(),
        granted_scopes_json: "[\"mcp\"]".to_string(),
        token_blob: vec![1, 2, 3, 4],
        token_blob_nonce: vec![0u8; 12],
        token_received_at: now,
        access_token_expires_at: now + 3600,
        refresh_token_present: true,
    }
}

fn sample_upstream_state() -> UpstreamOauthStateRow {
    let now = now_unix();
    UpstreamOauthStateRow {
        upstream_name: "acme".to_string(),
        subject: "alice".to_string(),
        csrf_token: "csrf-1".to_string(),
        pkce_verifier: "verifier-1".to_string(),
        created_at: now,
        expires_at: now + 300,
    }
}

#[tokio::test]
async fn sqlite_store_upsert_upstream_oauth_credentials_round_trip() {
    let store = temp_store().await;
    let row = sample_upstream_credentials();
    store
        .upsert_upstream_oauth_credentials(row.clone())
        .await
        .unwrap();

    let fetched = store
        .find_upstream_oauth_credentials("acme", "alice")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(fetched.upstream_name, row.upstream_name);
    assert_eq!(fetched.subject, row.subject);
    assert_eq!(fetched.client_id, row.client_id);
    assert_eq!(fetched.granted_scopes_json, row.granted_scopes_json);
    assert_eq!(fetched.token_blob, row.token_blob);
    assert_eq!(fetched.token_blob_nonce, row.token_blob_nonce);
    assert_eq!(fetched.token_received_at, row.token_received_at);
    assert_eq!(fetched.access_token_expires_at, row.access_token_expires_at);
    assert_eq!(fetched.refresh_token_present, row.refresh_token_present);
}

#[tokio::test]
async fn sqlite_store_takes_upstream_oauth_state_only_once_under_race() {
    let store = temp_store().await;
    store
        .save_upstream_oauth_state(sample_upstream_state())
        .await
        .unwrap();
    let now = now_unix();
    let (a, b) = tokio::join!(
        store.take_upstream_oauth_state("acme", "alice", "csrf-1", now),
        store.take_upstream_oauth_state("acme", "alice", "csrf-1", now),
    );
    let a_some = matches!(a, Ok(Some(_)));
    let b_some = matches!(b, Ok(Some(_)));
    assert!(
        a_some ^ b_some,
        "exactly one take should win: a={a:?} b={b:?}"
    );
}

#[tokio::test]
async fn sqlite_store_rejects_state_ttl_over_600s() {
    let store = temp_store().await;
    let mut row = sample_upstream_state();
    row.created_at = 1_000;
    row.expires_at = 1_000 + 601;
    let err = store.save_upstream_oauth_state(row).await.unwrap_err();
    assert!(err.to_string().contains("600"));
}

#[tokio::test]
async fn sqlite_store_cleanup_expired_drops_state() {
    let store = temp_store().await;
    let now = now_unix();
    let row = UpstreamOauthStateRow {
        upstream_name: "acme".to_string(),
        subject: "alice".to_string(),
        csrf_token: "csrf-expired".to_string(),
        pkce_verifier: "verifier-expired".to_string(),
        created_at: now - 400,
        expires_at: now - 10,
    };
    store.save_upstream_oauth_state(row).await.unwrap();

    store.cleanup_expired().await.unwrap();

    let fetched = store
        .take_upstream_oauth_state("acme", "alice", "csrf-expired", now)
        .await
        .unwrap();
    assert!(fetched.is_none(), "expired state should be gone");
}

#[tokio::test]
async fn sqlite_store_credentials_isolated_per_subject() {
    let store = temp_store().await;
    let mut row1 = sample_upstream_credentials();
    row1.subject = "alice".to_string();
    let mut row2 = sample_upstream_credentials();
    row2.subject = "bob".to_string();
    store.upsert_upstream_oauth_credentials(row1).await.unwrap();
    store.upsert_upstream_oauth_credentials(row2).await.unwrap();

    store
        .delete_upstream_oauth_credentials("acme", "alice")
        .await
        .unwrap();

    assert!(
        store
            .find_upstream_oauth_credentials("acme", "alice")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .find_upstream_oauth_credentials("acme", "bob")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn sqlite_store_upsert_overwrites_existing_credentials() {
    let store = temp_store().await;
    let row1 = sample_upstream_credentials();
    store.upsert_upstream_oauth_credentials(row1).await.unwrap();

    let mut row2 = sample_upstream_credentials();
    row2.client_id = "client-rotated".to_string();
    row2.token_blob = vec![9, 9, 9];
    store.upsert_upstream_oauth_credentials(row2).await.unwrap();

    let fetched = store
        .find_upstream_oauth_credentials("acme", "alice")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.client_id, "client-rotated");
    assert_eq!(fetched.token_blob, vec![9, 9, 9]);
}

#[tokio::test]
async fn dynamic_client_registration_round_trip() {
    let store = temp_store().await;

    // Nothing stored yet.
    assert!(
        store
            .find_dynamic_client_registration("acme", "alice")
            .await
            .unwrap()
            .is_none()
    );

    // Save and retrieve.
    store
        .save_dynamic_client_registration("acme", "alice", "client-dyn-1")
        .await
        .unwrap();
    let found = store
        .find_dynamic_client_registration("acme", "alice")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found, "client-dyn-1");

    // Upsert with a new client_id (server re-registered).
    store
        .save_dynamic_client_registration("acme", "alice", "client-dyn-2")
        .await
        .unwrap();
    let found2 = store
        .find_dynamic_client_registration("acme", "alice")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found2, "client-dyn-2");

    // Delete and confirm gone; other subjects unaffected.
    store
        .save_dynamic_client_registration("acme", "bob", "client-dyn-bob")
        .await
        .unwrap();
    store
        .delete_dynamic_client_registration("acme", "alice")
        .await
        .unwrap();
    assert!(
        store
            .find_dynamic_client_registration("acme", "alice")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .find_dynamic_client_registration("acme", "bob")
            .await
            .unwrap()
            .is_some()
    );
}
