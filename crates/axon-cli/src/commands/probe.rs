use axon_core::http::build_client;
// `with_path` moved to `core::http`; re-exported so existing
// `probe::with_path` call sites keep resolving.
pub use axon_core::http::with_path;

pub async fn probe_http(url: &str, paths: &[&str]) -> (bool, Option<String>) {
    if url.trim().is_empty() {
        return (false, Some("not configured".to_string()));
    }

    // Short 4s timeout for health probes — intentionally not the global 30s client.
    let client = match build_client(4, None) {
        Ok(c) => c,
        Err(err) => return (false, Some(err.to_string())),
    };

    let mut last_error = None;
    for path in paths {
        let endpoint = with_path(url, path);
        match client.get(endpoint).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() || status.is_redirection() {
                    return (true, Some(format!("http {}", status.as_u16())));
                }
                last_error = Some(format!("http {}", status.as_u16()));
            }
            Err(err) => last_error = Some(err.to_string()),
        }
    }

    (false, last_error)
}
