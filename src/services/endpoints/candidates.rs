use super::{probe, validate_url_with_dns_timeout};
use crate::services::types::{
    DiscoveredEndpoint, EndpointKind, EndpointReport, EndpointSourceKind, McpCandidateAttempt,
    McpHostKind, McpProbeOutcome, RpcProbeResult,
};
use futures_util::{StreamExt, stream};
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

/// Concurrency for synthesized-candidate probing (small fixed set; mirrors the
/// discovered-endpoint probe concurrency).
const SYNTH_PROBE_CONCURRENCY: usize = 4;

/// Synthesize MCP candidates from `target`, SSRF-validate, probe each with the
/// strict probe, append confirmed ones to `report.endpoints`, and record every
/// attempt in `report.mcp_candidates`.
///
/// Uses `buffer_unordered` (NOT `tokio::spawn`) so the `#[cfg(test)]` loopback
/// bypass thread-local propagates correctly.
pub(super) async fn synthesize_and_probe_mcp(
    client: &reqwest::Client,
    target: &str,
    include_subdomain: bool,
    report: &mut EndpointReport,
) {
    let candidates = mcp_candidate_urls(target, include_subdomain);
    // Dedup against already-discovered endpoints — those are probed by the
    // normal path; never double-probe.
    let candidates: Vec<Candidate> = candidates
        .into_iter()
        .filter(|c| {
            !report
                .endpoints
                .iter()
                .any(|e| e.normalized_url.as_deref() == Some(c.url.as_str()) || e.value == c.url)
        })
        .collect();

    let attempts: Vec<(Candidate, McpProbeOutcome, Option<RpcProbeResult>)> =
        stream::iter(candidates)
            .map(|c| {
                let client = client.clone();
                async move {
                    if validate_url_with_dns_timeout(&c.url).await.is_err() {
                        return (c, McpProbeOutcome::Blocked, None);
                    }
                    match probe::probe_candidate(&client, &c.url).await {
                        Some(rpc) => (c, McpProbeOutcome::Confirmed, Some(rpc)),
                        None => (c, McpProbeOutcome::Unconfirmed, None),
                    }
                }
            })
            .buffer_unordered(SYNTH_PROBE_CONCURRENCY)
            .collect()
            .await;

    for (c, outcome, rpc) in attempts {
        if outcome == McpProbeOutcome::Confirmed {
            if let Some(rpc) = rpc.clone() {
                report.endpoints.push(DiscoveredEndpoint {
                    value: c.url.clone(),
                    normalized_url: Some(c.url.clone()),
                    kind: EndpointKind::AbsoluteUrl,
                    // Same host or mcp.<apex-of-target> — same registrable org by
                    // construction.
                    first_party: true,
                    source: EndpointSourceKind::SynthesizedMcp,
                    source_url: Some(target.to_string()),
                    verified: None,
                    rpc_probe: Some(rpc),
                });
            }
        }
        report.mcp_candidates.push(McpCandidateAttempt {
            url: c.url,
            host_kind: c.host_kind,
            path: c.path.to_string(),
            outcome,
            rpc_probe: rpc,
        });
    }
}

#[cfg(test)]
#[path = "candidates_tests.rs"]
mod tests;
