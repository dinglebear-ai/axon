use super::*;

#[test]
fn readyz_url_uses_configured_host_and_port() {
    assert_eq!(
        axon_readyz_url("127.0.0.1", 8001),
        "http://127.0.0.1:8001/readyz"
    );
    assert_eq!(
        axon_readyz_url("axon.internal", 9090),
        "http://axon.internal:9090/readyz"
    );
}

#[test]
fn readyz_url_probes_bind_all_over_loopback() {
    for host in ["0.0.0.0", "::", "[::]", "*", "", "  "] {
        assert_eq!(
            axon_readyz_url(host, 8001),
            "http://127.0.0.1:8001/readyz",
            "bind-all host {host:?} should probe loopback"
        );
    }
}

#[test]
fn readyz_url_brackets_ipv6_literal() {
    assert_eq!(axon_readyz_url("::1", 8001), "http://[::1]:8001/readyz");
    // Already-bracketed host is left intact.
    assert_eq!(
        axon_readyz_url("[fe80::1]", 7000),
        "http://[fe80::1]:7000/readyz"
    );
}

#[test]
fn report_url_uses_configured_port_from_env_file() {
    let values = BTreeMap::from([
        ("AXON_HTTP_HOST".to_string(), "0.0.0.0".to_string()),
        ("AXON_HTTP_PORT".to_string(), "38123".to_string()),
    ]);
    assert_eq!(
        report_server_url_with(&values, |_| None),
        "http://127.0.0.1:38123"
    );
}

#[test]
fn compose_uses_external_qdrant_overlay_for_remote_url() {
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(
        &env_path,
        "QDRANT_URL=http://100.120.242.29:53333\nTEI_URL=http://127.0.0.1:52000\n",
    )
    .unwrap();

    assert_eq!(
        runtime::external_qdrant_url(&env_path).unwrap().as_deref(),
        Some("http://100.120.242.29:53333")
    );
}

#[test]
fn compose_keeps_bundled_qdrant_for_loopback_url() {
    let dir = tempfile::tempdir().unwrap();
    for url in [
        "http://127.0.0.1:53333",
        "http://localhost:53333",
        "http://axon-qdrant:6333",
    ] {
        let env_path = dir.path().join("loopback.env");
        std::fs::write(&env_path, format!("QDRANT_URL={url}\n")).unwrap();
        assert_eq!(runtime::external_qdrant_url(&env_path).unwrap(), None);
    }
}

#[test]
fn compose_network_defaults_to_axon() {
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, "QDRANT_URL=http://127.0.0.1:53333\n").unwrap();

    assert_eq!(
        runtime::compose_network_name(&env_path).unwrap(),
        "axon".to_string()
    );
}

#[test]
fn compose_network_uses_configured_name() {
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, "DOCKER_NETWORK=axon-isolated\n").unwrap();

    assert_eq!(
        runtime::compose_network_name(&env_path).unwrap(),
        "axon-isolated".to_string()
    );
}

#[test]
fn compose_uses_external_provider_overlay_when_both_urls_are_set() {
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(
        &env_path,
        "AXON_EXTERNAL_TEI_URL=http://host.docker.internal:52000\n\
         AXON_EXTERNAL_CHROME_REMOTE_URL=http://host.docker.internal:6000\n",
    )
    .unwrap();

    assert_eq!(
        runtime::external_provider_urls(&env_path).unwrap(),
        Some((
            "http://host.docker.internal:52000".to_string(),
            "http://host.docker.internal:6000".to_string()
        ))
    );
}

#[test]
fn compose_rejects_partial_external_provider_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(
        &env_path,
        "AXON_EXTERNAL_TEI_URL=http://host.docker.internal:52000\n",
    )
    .unwrap();

    let error = runtime::external_provider_urls(&env_path).unwrap_err();
    assert!(error.to_string().contains("must be set together"));
}
