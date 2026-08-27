fn is_loopback(host: &str) -> bool {
    use std::net::IpAddr;
    use std::str::FromStr;
    let h = host.trim();
    if h.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let h = h
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or(h);
    IpAddr::from_str(h)
        .map(|addr| addr.is_loopback())
        .unwrap_or(false)
}

#[test]
fn mcp_http_bind_loopback_detection_accepts_loopback_hosts() {
    assert!(is_loopback("127.0.0.1"));
    assert!(is_loopback("::1"));
    assert!(is_loopback("[::1]"));
    assert!(is_loopback("localhost"));
}

#[test]
fn mcp_http_bind_loopback_detection_rejects_wildcard_and_remote_hosts() {
    assert!(!is_loopback("0.0.0.0"));
    assert!(!is_loopback("::"));
    assert!(!is_loopback("192.168.1.10"));
    assert!(!is_loopback("axon.example.com"));
}

#[test]
fn mcp_http_uses_stateless_json_with_explicit_allowed_hosts() {
    let config = streamable_http_config(vec!["axon.example.com".to_string()]);

    assert!(!config.legacy_session_mode);
    assert!(config.json_response);
    assert_eq!(config.allowed_hosts, ["axon.example.com"]);
}
