use super::*;
use std::env;

fn sample(server: &str, refresh: Option<&str>, expires_at: i64) -> StoredCredentials {
    StoredCredentials {
        client_id: "client-123".to_string(),
        access_token: "access-abc".into(),
        refresh_token: refresh.map(Secret::from),
        token_endpoint: format!("{server}/token"),
        expires_at_unix: expires_at,
        scope: "axon:read axon:write".to_string(),
        server_url: server.to_string(),
    }
}

#[test]
fn save_then_load_round_trips() {
    let dir = env::temp_dir().join(format!("axon-oauth-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("oauth.json");

    let creds = sample(
        "https://axon.example.com",
        Some("refresh-xyz"),
        4_102_444_800,
    );
    save(&path, &creds).unwrap();
    let loaded = load(&path).expect("credentials present after save");

    assert_eq!(loaded.client_id, "client-123");
    assert_eq!(loaded.access_token.expose(), "access-abc");
    assert_eq!(
        loaded.refresh_token.as_ref().map(|s| s.expose()),
        Some("refresh-xyz")
    );
    assert_eq!(loaded.token_endpoint, "https://axon.example.com/token");
    assert_eq!(loaded.server_url, "https://axon.example.com");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_missing_file_returns_none() {
    let path = env::temp_dir().join(format!("axon-oauth-missing-{}.json", uuid::Uuid::new_v4()));
    assert!(load(&path).is_none());
}

#[test]
fn clear_removes_the_file_and_is_idempotent() {
    let dir = env::temp_dir().join(format!("axon-oauth-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("oauth.json");
    save(&path, &sample("https://a", None, 0)).unwrap();
    clear(&path).unwrap();
    assert!(load(&path).is_none());
    clear(&path).unwrap(); // second clear must not error
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn expiry_accounts_for_skew() {
    let creds = sample("https://a", None, 1000);
    assert!(!creds.is_expired(900, 30)); // 900 + 30 < 1000 → valid
    assert!(creds.is_expired(980, 30)); // 980 + 30 >= 1000 → treat as expired
    assert!(creds.is_expired(1000, 0));
}

#[test]
fn matches_server_is_exact_after_trailing_slash_trim() {
    let creds = sample("https://axon.example.com", None, 0);
    assert!(creds.matches_server("https://axon.example.com"));
    assert!(creds.matches_server("https://axon.example.com/"));
    assert!(!creds.matches_server("https://other.example.com"));
}

#[test]
fn debug_redacts_token_fields() {
    let creds = sample("https://axon.example.com", Some("refresh-xyz"), 0);
    let rendered = format!("{creds:?}");
    assert!(
        !rendered.contains("access-abc"),
        "access token leaked: {rendered}"
    );
    assert!(
        !rendered.contains("refresh-xyz"),
        "refresh token leaked: {rendered}"
    );
    assert!(
        rendered.contains("client-123"),
        "non-secret field should remain"
    );
}
