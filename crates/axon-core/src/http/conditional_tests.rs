use super::*;

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
    let h = conditional_headers(Some("\"a\""), Some("d"));
    assert!(h.iter().any(|(k, v)| k == "if-none-match" && v == "\"a\""));
    assert!(h.iter().any(|(k, v)| k == "if-modified-since" && v == "d"));
}
#[test]
fn headers_empty() {
    assert!(conditional_headers(None, None).is_empty());
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
