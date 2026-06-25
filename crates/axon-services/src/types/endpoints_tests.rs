use super::*;

#[test]
fn synthesized_mcp_source_kind_wire_string() {
    assert_eq!(
        EndpointSourceKind::SynthesizedMcp.as_str(),
        "synthesized_mcp"
    );
    let json = serde_json::to_string(&EndpointSourceKind::SynthesizedMcp).unwrap();
    assert_eq!(json, "\"synthesized_mcp\"");
}

#[test]
fn mcp_candidate_attempt_roundtrips() {
    // Non-contradictory state: Unconfirmed attempts carry no rpc_probe.
    let attempt = McpCandidateAttempt {
        url: "https://mcp.foo.com/mcp".to_string(),
        host_kind: McpHostKind::ApexSubdomain,
        path: "/mcp".to_string(),
        outcome: McpProbeOutcome::Unconfirmed,
        rpc_probe: None,
    };
    let json = serde_json::to_value(&attempt).unwrap();
    assert_eq!(json["host_kind"], "apex_subdomain");
    assert_eq!(json["outcome"], "unconfirmed");
    // rpc_probe is None → omitted
    assert!(json.get("rpc_probe").is_none());
}

#[test]
fn empty_mcp_candidates_omitted_from_report() {
    let report = EndpointReport {
        url: "https://x.test".to_string(),
        endpoints: Vec::new(),
        hosts: Vec::new(),
        scripts_discovered: 0,
        bundles_fetched: 0,
        bundles_scanned: 0,
        truncated: false,
        warnings: Vec::new(),
        elapsed_ms: 0,
        mcp_candidates: Vec::new(),
    };
    let json = serde_json::to_value(&report).unwrap();
    assert!(json.get("mcp_candidates").is_none());
}
