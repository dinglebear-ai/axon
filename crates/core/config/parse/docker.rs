use spider::url::Url;

/// Mapping from Docker-internal service hostnames to their host-side addresses.
///
/// These names only resolve within the Docker container network.  Outside Docker
/// (i.e. when `/.dockerenv` does not exist) each entry is rewritten to the
/// corresponding `localhost:PORT` so the host CLI can reach the service.
const HOST_MAP: &[(&str, &str, u16)] = &[
    ("axon-postgres", "127.0.0.1", 53432),
    ("axon-redis", "127.0.0.1", 53379),
    ("axon-rabbitmq", "127.0.0.1", 45535),
    ("axon-qdrant", "127.0.0.1", 53333),
    ("axon-tei", "127.0.0.1", 52000),
    ("axon-chrome", "127.0.0.1", 6000),
];

/// Returns `true` if `host` is a known Docker-internal service hostname.
///
/// These hostnames only resolve inside the Docker container network; outside
/// Docker they must be mapped to `127.0.0.1`.  Used by CDP URL normalisation
/// to rewrite WebSocket connection URLs returned by `headless_browser`.
pub(crate) fn is_docker_service_host(host: &str) -> bool {
    HOST_MAP.iter().any(|(h, _, _)| *h == host)
}

pub(crate) fn normalize_local_service_url(url: String) -> String {
    if std::path::Path::new("/.dockerenv").exists() {
        return url;
    }

    let Ok(mut parsed) = Url::parse(&url) else {
        return url;
    };
    let host = match parsed.host_str() {
        Some(h) => h.to_string(),
        None => return url,
    };
    for (container_host, local_host, local_port) in HOST_MAP {
        if host == *container_host {
            let _ = parsed.set_host(Some(local_host));
            let _ = parsed.set_port(Some(*local_port));
            return parsed.to_string();
        }
    }
    url
}
