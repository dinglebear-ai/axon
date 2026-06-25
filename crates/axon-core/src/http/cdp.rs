//! Chrome DevTools Protocol discovery URL construction.

use spider::url::Url;

/// Build the CDP `/json/version` discovery URL from a Chrome remote URL.
///
/// Handles `ws://` / `wss://` -> `http://` / `https://` conversion (reqwest cannot
/// make requests to `ws://` scheme URLs) and appends `/json/version` when the path
/// is absent or root.  Returns `None` if the URL cannot be parsed or uses an
/// unsupported scheme (`ftp://`, `file://`, etc.).
///
/// # Safety (SSRF)
///
/// This function performs **no SSRF validation** on the input URL. It trusts
/// that `remote_url` originates from a trusted configuration source (e.g.
/// `AXON_CHROME_REMOTE_URL` environment variable). Do **not** pass
/// user-controlled or untrusted URLs without first validating them through
/// [`super::ssrf::validate_url`].
pub fn cdp_discovery_url(remote_url: &str) -> Option<String> {
    let parsed = Url::parse(remote_url).ok()?;
    let http_scheme = match parsed.scheme() {
        "ws" | "http" => "http",
        "wss" | "https" => "https",
        _ => return None,
    };
    let host = parsed.host_str()?;
    let port = parsed.port_or_known_default()?;
    let path = parsed.path();
    let path = if path == "/" || path.is_empty() {
        "/json/version"
    } else {
        path
    };
    Some(format!("{http_scheme}://{host}:{port}{path}"))
}
