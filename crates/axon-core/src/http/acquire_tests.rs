use super::*;

/// The 380-byte body the four Akamai-fronted SC county sites actually return.
/// It carries no vendor sensor token, so it is detectable only by STATUS —
/// which is exactly why status-based escalation has to exist alongside
/// body-fingerprint detection.
const AKAMAI_ACCESS_DENIED: &str = r#"<HTML><HEAD>
<TITLE>Access Denied</TITLE>
</HEAD><BODY>
<H1>Access Denied</H1>
You don't have permission to access "http://www.example.gov/" on this server.<P>
Reference&#32;&#35;18.c5680117.1785282888.989fc064
</BODY></HTML>"#;

#[test]
fn block_like_statuses_are_narrow() {
    for status in [401, 403, 406, 429, 503] {
        assert!(
            is_block_like_status(status),
            "{status} should be block-like"
        );
    }
    // Escalating on these would fire a second BoringSSL request for every dead
    // link in a crawl.
    for status in [200, 301, 404, 410, 500, 502, 504] {
        assert!(
            !is_block_like_status(status),
            "{status} must NOT trigger escalation"
        );
    }
}

#[test]
fn akamai_denial_body_has_no_fingerprint_so_status_must_carry_it() {
    // Guards the reasoning above: if this body ever DID match a fingerprint the
    // status path would be redundant, and someone might remove it.
    assert!(
        detect_challenge(AKAMAI_ACCESS_DENIED, |_| None, DEFAULT_CHALLENGE_SCAN_BYTES).is_none(),
        "bare Akamai denial page carries no sensor token; status is the only signal"
    );
    assert!(is_block_like_status(403));
}

#[test]
fn html_bodies_are_fingerprint_scanned() {
    // The Akamai Bot Manager sensor token is the canonical fingerprint.
    let challenge = "<html><script>bazadebezolkohpepadr=1</script></html>";
    assert!(classify(challenge, &FetchWebOptions::html()).is_some());
    assert!(classify("<html>ordinary page</html>", &FetchWebOptions::html()).is_none());
}

#[test]
fn non_success_status_is_status_error_not_challenge() {
    let err = finish(
        String::new(),
        404,
        "https://example.com/x".into(),
        false,
        "https://example.com/x",
    )
    .expect_err("404 must not be Ok");
    assert!(
        matches!(err, FetchError::Status { status: 404, .. }),
        "404 must surface as Status, not Challenge: {err:?}"
    );
}

#[test]
fn success_status_produces_document() {
    let doc = finish(
        "<html/>".into(),
        200,
        "https://example.com/".into(),
        true,
        "https://example.com/",
    )
    .expect("200 must be Ok");
    assert_eq!(doc.status, 200);
    assert!(doc.escalated, "escalation flag must survive");
}

#[test]
fn challenge_error_names_the_vendor() {
    let err = FetchError::Challenge {
        url: "https://example.gov/".into(),
        status: 403,
        detection: None,
        escalation: EscalationOutcome::StillWalled,
    };
    let rendered = err.to_string();
    assert!(rendered.contains("https://example.gov/"), "{rendered}");
    assert!(rendered.contains("403"), "{rendered}");
}

#[test]
fn scan_budget_is_configurable_and_defaulted() {
    assert_eq!(
        FetchWebOptions::html()
            .with_scan_bytes(42)
            .challenge_scan_bytes,
        42
    );
    assert_eq!(
        FetchWebOptions::default().challenge_scan_bytes,
        DEFAULT_CHALLENGE_SCAN_BYTES
    );
}

#[test]
fn escalation_outcome_distinguishes_a_real_block_from_a_broken_retry() {
    // The whole point: an operator told "bot challenge" abandons the domain.
    // A transient escalation fault must not read the same way.
    let walled = FetchError::Challenge {
        url: "https://example.gov/".into(),
        status: 403,
        detection: None,
        escalation: EscalationOutcome::StillWalled,
    };
    let broke = FetchError::Challenge {
        url: "https://example.gov/".into(),
        status: 403,
        detection: None,
        escalation: EscalationOutcome::Failed("dns timeout".into()),
    };
    let missing = FetchError::Challenge {
        url: "https://example.gov/".into(),
        status: 403,
        detection: None,
        escalation: EscalationOutcome::Unavailable,
    };
    assert!(walled.to_string().contains("survived"), "{walled}");
    assert!(broke.to_string().contains("dns timeout"), "{broke}");
    assert!(
        missing.to_string().contains("tls-fingerprinting"),
        "operator must be told the feature is absent: {missing}"
    );
}

#[tokio::test]
async fn fetch_web_rejects_blocked_scheme_before_any_request() {
    let err = fetch_web("file:///etc/passwd", &FetchWebOptions::html())
        .await
        .expect_err("non-http scheme must be rejected");
    assert!(
        matches!(err, FetchError::Http(HttpError::BlockedScheme(_))),
        "expected BlockedScheme, got {err:?}"
    );
}

#[tokio::test]
async fn fetch_web_rejects_loopback_host() {
    let err = fetch_web("http://localhost/admin", &FetchWebOptions::html())
        .await
        .expect_err("loopback must be rejected");
    assert!(
        matches!(
            err,
            FetchError::Http(HttpError::BlockedHost(_) | HttpError::BlockedIpRange(_))
        ),
        "expected a blocked-host error, got {err:?}"
    );
}
