use crate::services::types::McpHostKind;
use url::Url;

/// Well-known MCP paths probed on each candidate host.
const MCP_PATHS: [&str; 2] = ["/mcp", "/api/mcp"];

/// One synthesized MCP candidate URL.
pub(super) struct Candidate {
    pub host_kind: McpHostKind,
    pub path: &'static str,
    pub url: String,
}

/// Registrable apex (eTLD+1) for `host` via the Public Suffix List.
/// Returns `None` for raw IPs, single-label hosts, and unknown suffixes.
pub(super) fn registrable_apex(host: &str) -> Option<String> {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if bare.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    psl::domain_str(host).map(|d| d.to_string())
}

/// Host[:port] authority of a parsed URL, lowercased host.
fn authority(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    Some(match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    })
}

/// Synthesize MCP candidate URLs from `target`.
///
/// Same-host candidates always use the target's scheme + authority. Subdomain
/// candidates (`mcp.<apex>`, https) are added only when `include_subdomain` and
/// an apex resolves, and are skipped when the seed host is already `mcp.*`
/// (they would duplicate the same-host set).
pub(super) fn mcp_candidate_urls(target: &str, include_subdomain: bool) -> Vec<Candidate> {
    let mut out = Vec::new();
    let Ok(url) = Url::parse(target) else {
        return out;
    };
    let scheme = url.scheme();
    let Some(auth) = authority(&url) else {
        return out;
    };
    for path in MCP_PATHS {
        out.push(Candidate {
            host_kind: McpHostKind::SameHost,
            path,
            url: format!("{scheme}://{auth}{path}"),
        });
    }

    if include_subdomain {
        if let Some(host) = url.host_str().map(|h| h.to_ascii_lowercase()) {
            if !host.starts_with("mcp.") {
                if let Some(apex) = registrable_apex(&host) {
                    for path in MCP_PATHS {
                        out.push(Candidate {
                            host_kind: McpHostKind::ApexSubdomain,
                            path,
                            url: format!("https://mcp.{apex}{path}"),
                        });
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "candidates_tests.rs"]
mod tests;
