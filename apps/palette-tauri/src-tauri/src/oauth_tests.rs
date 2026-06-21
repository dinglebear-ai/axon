use super::*;
use crate::oauth::store::StoredCredentials;

fn creds(server: &str) -> StoredCredentials {
    StoredCredentials {
        client_id: "c".to_string(),
        access_token: "a".into(),
        refresh_token: None,
        token_endpoint: format!("{server}/token"),
        expires_at_unix: 4_102_444_800,
        scope: "axon:read axon:write".to_string(),
        server_url: server.to_string(),
    }
}

#[test]
fn pick_token_prefers_oauth_then_static() {
    assert_eq!(
        pick_token(Some("oauth".to_string()), Some("static".to_string())),
        Some("oauth".to_string())
    );
    assert_eq!(
        pick_token(None, Some("static".to_string())),
        Some("static".to_string())
    );
    assert_eq!(pick_token(None, None), None);
}

#[test]
fn status_for_reports_signed_in_only_when_server_matches() {
    let c = creds("https://axon.example.com");

    let matched = status_for(Some(&c), "https://axon.example.com");
    assert!(matched.signed_in);
    assert_eq!(matched.scope.as_deref(), Some("axon:read axon:write"));

    // Credentials for a different server → not signed in here, but the stored
    // server_url is surfaced so the UI can explain the mismatch.
    let mismatched = status_for(Some(&c), "https://other.example.com");
    assert!(!mismatched.signed_in);
    assert_eq!(
        mismatched.server_url.as_deref(),
        Some("https://axon.example.com")
    );

    let none = status_for(None, "https://axon.example.com");
    assert!(!none.signed_in);
    assert!(none.server_url.is_none());
}

#[test]
fn credentials_from_token_clamps_huge_expires_in_and_trims_server_url() {
    let token: crate::oauth::flow::TokenResponse = serde_json::from_str(
        r#"{"access_token":"a","token_type":"Bearer","expires_in":18446744073709551615,"scope":"axon:read"}"#,
    )
    .unwrap();
    let creds = credentials_from_token(
        "c".to_string(),
        "https://x/",
        "https://x/token".to_string(),
        None,
        token,
        1000,
    );
    assert!(
        creds.expires_at_unix > 1000,
        "huge expires_in must not wrap negative"
    );
    assert_eq!(creds.server_url, "https://x", "trailing slash trimmed");
    assert!(
        creds.refresh_token.is_none(),
        "absent refresh_token stays None"
    );
}

#[test]
fn classify_refresh_maps_each_result_to_the_right_outcome() {
    let ok: crate::oauth::flow::TokenResponse = serde_json::from_str(
        r#"{"access_token":"a","token_type":"Bearer","expires_in":3600,"scope":"s"}"#,
    )
    .unwrap();
    assert!(matches!(
        classify_refresh(
            Ok(ok),
            "c".to_string(),
            "https://x",
            "https://x/token".to_string(),
            None,
            1000
        ),
        RefreshOutcome::Refreshed(_)
    ));
    assert!(matches!(
        classify_refresh(
            Err(crate::oauth::flow::TokenError {
                rejected: true,
                message: String::new()
            }),
            "c".to_string(),
            "https://x",
            "t".to_string(),
            None,
            1000
        ),
        RefreshOutcome::Cleared
    ));
    assert!(matches!(
        classify_refresh(
            Err(crate::oauth::flow::TokenError {
                rejected: false,
                message: String::new()
            }),
            "c".to_string(),
            "https://x",
            "t".to_string(),
            None,
            1000
        ),
        RefreshOutcome::Kept
    ));
}

#[test]
fn credentials_from_token_preserves_prior_refresh_token_when_omitted() {
    // Response WITHOUT a refresh_token (provider reuses the existing one).
    let token: crate::oauth::flow::TokenResponse = serde_json::from_str(
        r#"{"access_token":"a","token_type":"Bearer","expires_in":3600,"scope":"s"}"#,
    )
    .unwrap();
    let prior = Some(crate::oauth::secret::Secret::from("prior-refresh"));
    let creds = credentials_from_token(
        "c".to_string(),
        "https://x",
        "https://x/token".to_string(),
        prior,
        token,
        1000,
    );
    assert_eq!(
        creds.refresh_token.as_ref().map(|s| s.expose()),
        Some("prior-refresh")
    );

    // Response WITH a refresh_token overrides the prior.
    let token: crate::oauth::flow::TokenResponse = serde_json::from_str(
        r#"{"access_token":"a","token_type":"Bearer","expires_in":3600,"refresh_token":"new-refresh","scope":"s"}"#,
    )
    .unwrap();
    let prior = Some(crate::oauth::secret::Secret::from("prior-refresh"));
    let creds = credentials_from_token(
        "c".to_string(),
        "https://x",
        "https://x/token".to_string(),
        prior,
        token,
        1000,
    );
    assert_eq!(
        creds.refresh_token.as_ref().map(|s| s.expose()),
        Some("new-refresh")
    );
}
