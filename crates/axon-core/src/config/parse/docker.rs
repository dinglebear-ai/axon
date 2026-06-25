use spider::url::Url;

use crate::logging::log_warn;

/// Returns `true` when the process is running inside a container.
///
/// Checks in priority order:
/// 1. `AXON_IN_CONTAINER` env var set to a truthy value (`1`, `true`, `TRUE`,
///    `yes`, `YES`) — testable without touching the filesystem; bake this into
///    your Dockerfile via `ENV AXON_IN_CONTAINER=1`.
/// 2. `/.dockerenv` — present in every Docker container.
/// 3. `/run/.containerenv` — present in Podman rootless containers.
///
/// The env-var check is re-evaluated on every call (no caching) so that tests
/// can safely set/unset `AXON_IN_CONTAINER` without fighting a stale
/// `LazyLock`.  The filesystem checks are microsecond-level `stat()` calls
/// and are only reached when the env var is unset.
pub fn running_in_container() -> bool {
    if let Ok(value) = std::env::var("AXON_IN_CONTAINER") {
        return matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES");
    }
    std::path::Path::new("/.dockerenv").exists()
        || std::path::Path::new("/run/.containerenv").exists()
}

/// Mapping from Docker-internal service hostnames to their host-side addresses.
///
/// These names only resolve within the Docker container network.  Outside Docker
/// (i.e. when `/.dockerenv` does not exist) each entry is rewritten to the
/// corresponding `localhost:PORT` so the host CLI can reach the service.
const HOST_MAP: &[(&str, &str, u16)] = &[
    ("axon-qdrant", "127.0.0.1", 53333),
    ("axon-tei", "127.0.0.1", 52000),
    ("axon-ollama", "127.0.0.1", 11434),
    ("axon-chrome", "127.0.0.1", 6000),
];

/// Returns `true` if `host` is a known Docker-internal service hostname.
///
/// These hostnames only resolve inside the Docker container network; outside
/// Docker they must be mapped to `127.0.0.1`.  Used by CDP URL normalisation
/// to rewrite WebSocket connection URLs returned by `headless_browser`.
pub fn is_docker_service_host(host: &str) -> bool {
    HOST_MAP.iter().any(|(h, _, _)| *h == host)
}

pub(crate) fn normalize_local_service_url(url: String) -> String {
    if running_in_container() {
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
            if parsed.set_host(Some(local_host)).is_err() {
                log_warn(&format!(
                    "docker_url_rewrite action=set_host_failed source_host={host} target_host={local_host}"
                ));
                return url;
            }
            if parsed.set_port(Some(*local_port)).is_err() {
                log_warn(&format!(
                    "docker_url_rewrite action=set_port_failed url={url} target_port={local_port}"
                ));
                return url;
            }
            return parsed.to_string();
        }
    }
    url
}

#[cfg(test)]
#[path = "docker_tests.rs"]
mod tests;
