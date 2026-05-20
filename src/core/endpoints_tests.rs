use super::{EndpointKind, EndpointSource, resolve_host_endpoint};

#[test]
fn container_dns_qdrant_uses_localhost_candidate_for_host_runtime() {
    let resolved =
        resolve_host_endpoint(EndpointKind::Qdrant, Some("http://axon-qdrant:6333"), &[])
            .expect("resolved endpoint");

    assert_eq!(resolved.url, "http://127.0.0.1:53333");
    assert_eq!(resolved.source, EndpointSource::LocalhostDefault);
    assert!(resolved.warnings[0].contains("container DNS"));
}

#[test]
fn host_valid_config_url_wins_over_default() {
    let resolved = resolve_host_endpoint(
        EndpointKind::Embedding,
        Some("http://192.168.1.20:52000"),
        &[],
    )
    .expect("resolved endpoint");

    assert_eq!(resolved.url, "http://192.168.1.20:52000");
    assert_eq!(resolved.source, EndpointSource::Configured);
}

#[test]
fn hostname_substring_does_not_count_as_container_dns() {
    let resolved = resolve_host_endpoint(
        EndpointKind::Qdrant,
        Some("http://not-axon-qdrant.example:6333"),
        &[],
    )
    .expect("resolved endpoint");

    assert_eq!(resolved.url, "http://not-axon-qdrant.example:6333");
    assert_eq!(resolved.source, EndpointSource::Configured);
    assert!(resolved.warnings.is_empty());
}

#[test]
fn container_dns_llm_uses_trusted_cached_fallback_when_no_localhost_default_exists() {
    let resolved = resolve_host_endpoint(
        EndpointKind::Llm,
        Some("http://axon-llm:11434"),
        &["http://127.0.0.1:11434".to_string()],
    )
    .expect("trusted endpoint");

    assert_eq!(resolved.url, "http://127.0.0.1:11434");
    assert_eq!(resolved.source, EndpointSource::TrustedCached);
    assert!(resolved.warnings[0].contains("trusted cached"));
}
