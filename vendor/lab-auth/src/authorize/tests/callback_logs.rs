use super::*;
use std::io::Write;
use std::sync::{Arc, Mutex};
use tracing::instrument::WithSubscriber;

#[derive(Clone)]
struct CapturedLog(Arc<Mutex<Vec<u8>>>);

impl Write for CapturedLog {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn oauth_client_callback_logs_fingerprint_without_redirect_credentials() {
    let state = test_auth_state_with_mock_google().await;
    let captured = CapturedLog(Arc::new(Mutex::new(Vec::new())));
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter("lab_auth=debug")
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/auth/google/callback?state=good-state&code=upstream-code")
                .body(Body::empty())
                .unwrap(),
        )
        .with_subscriber(subscriber)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let redirect = Url::parse(response.headers()[header::LOCATION].to_str().unwrap()).unwrap();
    assert_eq!(
        redirect.origin().ascii_serialization(),
        "http://127.0.0.1:7777"
    );
    assert_eq!(redirect.path(), "/callback");
    let pairs: std::collections::HashMap<_, _> = redirect.query_pairs().collect();
    let code = pairs
        .get("code")
        .expect("client must receive its authorization code");
    assert!(!code.is_empty());
    assert_eq!(
        pairs.get("state").map(|value| value.as_ref()),
        Some("client-state")
    );
    let logs = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(
        logs.contains("oauth callback redirecting client back to registered callback"),
        "capture must observe actual callback debug event"
    );
    assert!(
        logs.contains(&crate::util::fingerprint(code)),
        "safe code fingerprint remains available for correlation"
    );
    for secret in [
        code.as_ref(),
        "client-state",
        "good-state",
        "upstream-code",
        "refresh-token",
        "google-access-token",
        "client-secret",
        "provider-verifier",
    ] {
        assert!(
            !logs.contains(secret),
            "OAuth credential or state leaked into callback logs"
        );
    }
}
