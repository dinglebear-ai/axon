//! Classification for renderer failures.
/// Classification of a `scrape_to_result` failure, derived from its
/// `Box<dyn Error>` message text — the underlying axon-crawl error carries no
/// typed status to match on (unlike `HttpFetchProvider`, which classifies a
/// real `reqwest::StatusCode`). Mirrors the same three-way health mapping: a
/// transient timeout is `Degraded`, a rate-limited response is `Cooling`,
/// everything else (5xx, connection failure, SSRF rejection) is `Unavailable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderFailureClass {
    Timeout,
    RateLimited,
    Transient,
    Fatal,
}

pub(crate) fn classify_render_error(message: &str) -> RenderFailureClass {
    let lower = message.to_ascii_lowercase();
    if lower.contains("http 429") || lower.contains("rate limit") {
        RenderFailureClass::RateLimited
    } else if lower.contains("timeout") || lower.contains("timed out") {
        RenderFailureClass::Timeout
    } else if lower.contains("http 5") {
        RenderFailureClass::Transient
    } else {
        RenderFailureClass::Fatal
    }
}
