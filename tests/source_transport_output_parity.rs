//! Behavioral source-result parity across CLI JSON, MCP payloads, and REST's
//! transport-neutral `Json<SourceResult>` contract (issue #465 / T7).

use axon_core::config::Config;

#[test]
fn source_result_json_is_identical_across_transports() {
    let result = axon_services::source::result_map::unsupported_result(
        "https://example.com/parity",
        "parity fixture",
    );
    let canonical = serde_json::to_value(&result).expect("serialize SourceResult");

    let cli = axon_cli::commands::source::source_result_json(&Config::test_default(), &result);
    let mcp = axon_mcp::server::source_result_payload(&result);
    // REST's `/v1/sources` handler returns `Json<SourceResult>` directly, so
    // serde's canonical SourceResult representation is the REST body shape.
    let rest = serde_json::to_value(&result).expect("serialize REST SourceResult body");

    assert_eq!(
        cli, canonical,
        "CLI --json must be the shared SourceResult DTO"
    );
    assert_eq!(
        mcp, canonical,
        "MCP must return the shared SourceResult DTO"
    );
    assert_eq!(
        rest, canonical,
        "REST must return the shared SourceResult DTO"
    );

    // Guard the specific drift that motivated T7: transport-only aliases must
    // not reappear while canonical source/generation/result fields remain.
    assert!(canonical.get("source_id").is_some());
    assert!(canonical.get("ledger").is_some());
    assert!(canonical.get("counts").is_some());
    assert!(canonical.get("generation").is_none());
    assert!(canonical.get("documents_prepared").is_none());
    assert!(canonical.get("vector_points_written").is_none());
}
