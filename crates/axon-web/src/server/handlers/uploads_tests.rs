use super::*;

#[test]
fn digest_header_accepts_canonical_hex_without_path_or_body_fallbacks() {
    let digest = "a".repeat(64);
    let mut headers = HeaderMap::new();
    headers.insert("x-content-sha256", digest.parse().unwrap());
    assert_eq!(upload_sha256_header(&headers).unwrap(), Some(digest));
}

#[test]
fn malformed_digest_header_fails_closed() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-content-sha256",
        axum::http::HeaderValue::from_bytes(b"\xff").unwrap(),
    );
    assert!(upload_sha256_header(&headers).is_err());
}

#[test]
fn standard_digest_header_is_decoded_to_canonical_hex() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "digest",
        "sha-256=:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=:"
            .parse()
            .unwrap(),
    );
    assert_eq!(
        upload_sha256_header(&headers).unwrap(),
        Some("00".repeat(32))
    );
}
