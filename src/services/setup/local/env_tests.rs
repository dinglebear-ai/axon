use super::*;

#[test]
fn env_file_preserves_existing_secrets_and_adds_missing_runtime_keys() {
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(
        &env_path,
        "AXON_MCP_HTTP_TOKEN=keep-me\nTAVILY_API_KEY=also-keep\n",
    )
    .unwrap();

    ensure_env_file(&env_path).unwrap();
    let raw = std::fs::read_to_string(&env_path).unwrap();
    assert!(raw.contains("AXON_MCP_HTTP_TOKEN=keep-me"));
    assert!(raw.contains("TAVILY_API_KEY=also-keep"));
    assert!(raw.contains("TEI_EMBEDDING_MODEL=Qwen/Qwen3-Embedding-0.6B"));
}

#[test]
fn env_file_repairs_blank_token() {
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, "AXON_MCP_HTTP_TOKEN=   \n").unwrap();

    ensure_env_file(&env_path).unwrap();
    let raw = std::fs::read_to_string(&env_path).unwrap();
    assert!(!raw.contains("AXON_MCP_HTTP_TOKEN=   "));
    assert!(raw.contains("AXON_MCP_HTTP_TOKEN="));
}

#[test]
fn parse_env_file_ignores_comments_and_empty_lines() {
    let parsed = parse_env_file("\n# comment\nA=1\nB = 'two words'\n").unwrap();
    assert_eq!(parsed.get("A").map(String::as_str), Some("1"));
    assert_eq!(parsed.get("B").map(String::as_str), Some("two words"));
}

#[test]
fn parse_env_file_decodes_quoted_oauth_mode_like_runtime() {
    let parsed = parse_env_file("AXON_MCP_AUTH_MODE=\"oauth\"\n").unwrap();
    assert_eq!(
        parsed.get("AXON_MCP_AUTH_MODE").map(String::as_str),
        Some("oauth")
    );
}

#[test]
fn write_env_file_quotes_dotenvy_sensitive_values() {
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    let env = BTreeMap::from([
        ("PLAIN".to_string(), "value".to_string()),
        ("SPACED".to_string(), "two words".to_string()),
        ("APOSTROPHE".to_string(), "Let's go".to_string()),
    ]);

    write_env_file(&env_path, &env).unwrap();

    let raw = std::fs::read_to_string(&env_path).unwrap();
    assert!(raw.contains("PLAIN=value"));
    assert!(raw.contains("SPACED='two words'"));
    assert!(raw.contains("APOSTROPHE='Let'\\''s go'"));
    let parsed = parse_env_file(&raw).unwrap();
    assert_eq!(
        parsed.get("APOSTROPHE").map(String::as_str),
        Some("Let's go")
    );
}

#[test]
fn explicit_process_token_replaces_existing_env_token_only() {
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(
        &env_path,
        "AXON_MCP_HTTP_TOKEN=old-token\nTAVILY_API_KEY=keep-this\n",
    )
    .unwrap();

    let result = ensure_env_file_with_process(
        &env_path,
        |key| (key == "AXON_MCP_HTTP_TOKEN").then(|| "plugin-token".to_string()),
        &EnvSetupOptions::default(),
    )
    .unwrap();
    let raw = std::fs::read_to_string(&env_path).unwrap();
    assert_eq!(
        result.values.get("AXON_MCP_HTTP_TOKEN").map(String::as_str),
        Some("plugin-token")
    );
    assert!(raw.contains("AXON_MCP_HTTP_TOKEN=plugin-token"));
    assert!(!raw.contains("old-token"));
    assert!(raw.contains("TAVILY_API_KEY=keep-this"));
    assert!(!result.phase.detail.contains("plugin-token"));
}

#[test]
fn env_example_host_urls_are_loopback_not_container_dns() {
    let parsed = parse_env_file(include_str!("../../../../.env.example")).unwrap();
    for key in ["QDRANT_URL", "TEI_URL", "AXON_CHROME_REMOTE_URL"] {
        let value = parsed.get(key).expect("template key exists");
        assert!(
            value.starts_with("http://127.0.0.1:"),
            "{key} must be host loopback, got {value}"
        );
        assert!(
            !value.contains("axon-qdrant")
                && !value.contains("axon-tei")
                && !value.contains("axon-chrome"),
            "{key} must not use Docker DNS in host template"
        );
    }
}
