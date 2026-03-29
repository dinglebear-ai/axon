use super::AcpMcpServerConfig;

#[test]
fn sse_variant_name_returns_name() {
    let cfg = AcpMcpServerConfig::Sse {
        name: "my-sse".to_string(),
        url: "http://localhost:3000/sse".to_string(),
        headers: vec![],
    };
    assert_eq!(cfg.name(), "my-sse");
}

#[test]
fn http_variant_with_headers_roundtrips_serde() {
    let cfg = AcpMcpServerConfig::Http {
        name: "my-http".to_string(),
        url: "http://localhost:3000/mcp".to_string(),
        headers: vec![("Authorization".to_string(), "Bearer tok".to_string())],
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let roundtrip: AcpMcpServerConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg, roundtrip);
}

#[test]
fn sse_variant_roundtrips_serde() {
    let cfg = AcpMcpServerConfig::Sse {
        name: "my-sse".to_string(),
        url: "http://localhost:3000/sse".to_string(),
        headers: vec![("X-Api-Key".to_string(), "secret".to_string())],
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let roundtrip: AcpMcpServerConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg, roundtrip);
}
