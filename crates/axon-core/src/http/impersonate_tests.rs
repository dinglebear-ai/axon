use super::*;

#[test]
fn chrome_profile_builds_without_panicking() {
    // The TLS/H2 builders take wire-format strings that BoringSSL parses at
    // client-build time; a typo in a cipher or curve name surfaces here.
    let _ = chrome_tls();
    let _ = chrome_h2();
    assert_eq!(chrome_extensions().len(), 16, "Chrome extension count");
}

#[test]
fn impersonating_client_builds() {
    assert!(
        impersonating_client().is_ok(),
        "built-in Chrome profile must produce a usable client"
    );
}

#[test]
fn impersonating_clients_are_request_scoped() {
    let first = impersonating_client().expect("first impersonating client");
    let second = impersonating_client().expect("second impersonating client");

    assert!(
        !std::ptr::eq(&first, &second),
        "arbitrary-host escalations must not share a cookie jar"
    );
}

#[test]
fn chrome_headers_are_all_valid_header_pairs() {
    for (name, value) in CHROME_HEADERS {
        assert!(
            wreq::header::HeaderName::from_bytes(name.as_bytes()).is_ok(),
            "invalid header name: {name}"
        );
        assert!(
            wreq::header::HeaderValue::from_str(value).is_ok(),
            "invalid header value for {name}"
        );
    }
}

#[test]
fn curves_put_post_quantum_group_first() {
    // X25519MLKEM768 leading is the modern-Chrome tell; losing it silently
    // reverts the fingerprint to an older Chrome that WAFs may score worse.
    assert!(
        CHROME_CURVES.starts_with("X25519MLKEM768:"),
        "post-quantum group must lead: {CHROME_CURVES}"
    );
}

#[test]
fn h2_settings_omit_max_concurrent_streams() {
    // Real Chrome omits MAX_CONCURRENT_STREAMS; its presence is a bot signal.
    // A setting is only emitted when it has a value, so assert the VALUE is
    // unset — `settings_order` enumerates every known id regardless and is not
    // the thing that decides what goes on the wire.
    let dbg = format!("{:?}", chrome_h2());
    assert!(
        dbg.contains("max_concurrent_streams: None"),
        "MAX_CONCURRENT_STREAMS must stay unset: {dbg}"
    );
    // The settings we DO send, and their values.
    for expected in [
        "header_table_size: Some(65536)",
        "enable_push: Some(false)",
        "initial_window_size: 6291456",
        "max_header_list_size: Some(262144)",
    ] {
        assert!(dbg.contains(expected), "missing {expected} in {dbg}");
    }
}

#[test]
fn h2_headers_priority_frame_matches_chrome() {
    // weight 255 (+1 = 256) exclusive, depending on stream 0. This is the field
    // that most directly moves the Akamai HTTP/2 fingerprint hash.
    let dbg = format!("{:?}", chrome_h2());
    assert!(
        dbg.contains(
            "StreamDependency { dependency_id: StreamId(0), weight: 255, is_exclusive: true }"
        ),
        "HEADERS priority frame drifted: {dbg}"
    );
}

#[tokio::test]
async fn impersonated_fetch_rejects_blocked_scheme() {
    let err = fetch_html_impersonated("file:///etc/passwd")
        .await
        .expect_err("non-http scheme must be rejected");
    assert!(
        matches!(err, HttpError::BlockedScheme(_)),
        "expected BlockedScheme, got {err:?}"
    );
}

#[tokio::test]
async fn impersonated_fetch_rejects_loopback_host() {
    // Parse-time SSRF validation must apply to the impersonating path exactly
    // as it does to the shared reqwest client — the fingerprinting client is
    // not an SSRF escape hatch.
    let err = fetch_html_impersonated("http://localhost/admin")
        .await
        .expect_err("loopback host must be rejected");
    assert!(
        matches!(
            err,
            HttpError::BlockedHost(_) | HttpError::BlockedIpRange(_)
        ),
        "expected a blocked-host error, got {err:?}"
    );
}
