use super::*;

#[test]
fn host_allowlist_accepts_loopback_and_configured_origin_authority() {
    let allowlist =
        HostAllowlist::new("127.0.0.1", 8001, &["https://axon.example.com".to_string()]);

    assert!(allowlist.allows("localhost:8001"));
    assert!(allowlist.allows("axon.example.com"));
    assert!(!allowlist.allows("evil.example.com"));
}
