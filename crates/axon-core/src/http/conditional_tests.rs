use super::*;
use crate::http::LoopbackGuard;
use httpmock::prelude::*;

#[test]
fn classify_304() {
    assert_eq!(classify(304, None, None), Probe::NotModified);
}
#[test]
fn classify_200() {
    assert_eq!(
        classify(200, Some("\"a\"".into()), Some("d".into())),
        Probe::Modified {
            etag: Some("\"a\"".into()),
            last_modified: Some("d".into())
        }
    );
}
#[test]
fn classify_500_failed() {
    match classify(500, None, None) {
        Probe::Failed(m) => assert!(m.contains("500")),
        o => panic!("{o:?}"),
    }
}
#[test]
fn headers_present() {
    let h = conditional_headers("https://example.com/page", Some("\"a\""), Some("d"));
    assert!(h.iter().any(|(k, v)| k == "if-none-match" && v == "\"a\""));
    assert!(h.iter().any(|(k, v)| k == "if-modified-since" && v == "d"));
}
#[test]
fn headers_empty() {
    assert!(conditional_headers("https://example.com/page", None, None).is_empty());
}

#[test]
fn cleartext_probe_drops_resource_validators() {
    assert!(
        conditional_headers(
            "http://example.com/page",
            Some("private-etag"),
            Some("private-modified-date"),
        )
        .is_empty()
    );
}

#[tokio::test]
async fn conditional_probe_does_not_follow_http_redirects() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    let destination = server
        .mock_async(|when, then| {
            when.method(GET).path("/destination");
            then.status(200);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/start");
            then.status(302)
                .header("location", server.url("/destination"));
        })
        .await;

    let probe = conditional_probe(&server.url("/start"), Some("private-etag"), None).await;

    assert_eq!(
        probe,
        Probe::Failed("conditional probe got HTTP 302".to_string())
    );
    destination.assert_calls_async(0).await;
}

#[tokio::test]
async fn rejected_probe_does_not_echo_credentialed_url() {
    let secret = "credential-value";
    let probe = conditional_probe(
        &format!("http://user:{secret}@127.0.0.1/private?api_key={secret}"),
        None,
        None,
    )
    .await;
    let Probe::Failed(message) = probe else {
        panic!("private target must be rejected")
    };
    assert!(!message.contains(secret));
}
