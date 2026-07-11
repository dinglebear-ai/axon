//! WS-H / audit D1 (issue #298): cross-surface auth-scope consistency
//! matrix.
//!
//! For each operation present on both REST and MCP, this asserts the two
//! transports agree on the auth-scope *class* (`public`/`read`/`write`/
//! `admin`), cross-checked against
//! `docs/pipeline-unification/surfaces/tool-contract.md`'s "Auth and
//! Visibility" operation-class table. Expected values live in
//! `tests/fixtures/cross-surface/scope_matrix.json` — every row (including
//! documented `consistent: false` rows) is checked against live system
//! state, so scope drift in either direction fails the test rather than
//! silently passing.

use axon_mcp::server::required_scope_for;
use axon_services::types::{RestRouteAuth, rest_route_inventory};
use serde::Deserialize;

const FIXTURE: &str = include_str!("fixtures/cross-surface/scope_matrix.json");

#[derive(Debug, Deserialize)]
struct ScopeMatrixFixture {
    rows: Vec<ScopeRow>,
}

#[derive(Debug, Deserialize)]
struct ScopeRow {
    op: String,
    mcp_action: String,
    mcp_subaction: String,
    mcp_scope: String,
    rest_method: String,
    rest_path: String,
    rest_scope: String,
    consistent: bool,
    note: String,
}

/// Normalize `required_scope_for`'s raw `Option<&str>` return into the
/// fixture's shared vocabulary (`public`/`read`/`write`/`admin`/`unknown`).
fn mcp_scope_class(action: &str, subaction: &str) -> &'static str {
    match required_scope_for(action, subaction) {
        None => "public",
        Some("axon:read") => "read",
        Some("axon:write") => "write",
        Some("axon:admin") => "admin",
        Some("__deny__") => "unknown",
        Some(other) => panic!("unrecognized MCP scope string `{other}` for {action}/{subaction}"),
    }
}

/// Normalize `RestRouteAuth` into the same shared vocabulary.
fn rest_scope_class(auth: RestRouteAuth) -> &'static str {
    match auth {
        RestRouteAuth::Public => "public",
        RestRouteAuth::Read => "read",
        RestRouteAuth::Write => "write",
        RestRouteAuth::Admin => "admin",
    }
}

fn rest_scope_for(method: &str, path: &str) -> &'static str {
    let route = rest_route_inventory()
        .iter()
        .find(|route| route.method == method && route.path == path)
        .unwrap_or_else(|| panic!("REST route inventory is missing {method} {path}"));
    rest_scope_class(route.auth)
}

#[test]
fn scope_matrix_fixture_is_not_vacuous() {
    let fixture: ScopeMatrixFixture =
        serde_json::from_str(FIXTURE).expect("parse scope_matrix.json fixture");
    assert!(
        fixture.rows.len() >= 10,
        "scope matrix fixture looks too small to be meaningful"
    );
    let consistent_rows = fixture.rows.iter().filter(|row| row.consistent).count();
    let divergent_rows = fixture.rows.iter().filter(|row| !row.consistent).count();
    assert!(
        consistent_rows >= 10,
        "expected most operations to have matching REST/MCP scope classes"
    );
    assert!(
        divergent_rows >= 3,
        "expected at least a few KNOWN_DIVERGENCE scope rows (query-shaped LLM surfaces); \
         matrix may be miscoded"
    );
}

#[test]
fn rest_and_mcp_scope_classes_match_fixture_for_every_operation() {
    let fixture: ScopeMatrixFixture =
        serde_json::from_str(FIXTURE).expect("parse scope_matrix.json fixture");

    for row in &fixture.rows {
        let actual_mcp = mcp_scope_class(&row.mcp_action, &row.mcp_subaction);
        let actual_rest = rest_scope_for(&row.rest_method, &row.rest_path);

        assert_eq!(
            actual_mcp, row.mcp_scope,
            "operation `{}`: MCP scope class drifted from fixture (expected {}, got {}) \
             for action={} subaction={:?}. {}",
            row.op, row.mcp_scope, actual_mcp, row.mcp_action, row.mcp_subaction, row.note
        );
        assert_eq!(
            actual_rest, row.rest_scope,
            "operation `{}`: REST scope class drifted from fixture (expected {}, got {}) \
             for {} {}. {}",
            row.op, row.rest_scope, actual_rest, row.rest_method, row.rest_path, row.note
        );

        let actually_consistent = actual_mcp == actual_rest;
        assert_eq!(
            row.consistent, actually_consistent,
            "operation `{}`: fixture `consistent` flag ({}) does not match actual \
             mcp={} vs rest={} scope comparison — fix the fixture row itself",
            row.op, row.consistent, actual_mcp, actual_rest
        );
        if !row.consistent {
            assert!(
                row.note.contains("KNOWN_DIVERGENCE"),
                "operation `{}` is flagged scope-inconsistent but has no KNOWN_DIVERGENCE \
                 note explaining/tracking it",
                row.op
            );
        }
    }
}

/// Tool-contract.md's own "Auth and Visibility" table states that
/// query/retrieve/ask/search/research/summarize are `axon:read` operation
/// classes. Lock that the MCP side actually matches the doc (independent of
/// the REST-side KNOWN_DIVERGENCE rows above, which track where REST hasn't
/// caught up to the documented class yet).
///
/// This checks the *nominal* class only. `search`/`research` additionally
/// upgrade to `axon:write` at dispatch time via `mutates_if_upgrade` (see
/// `tests/mcp_contract_parity.rs::mutates_if_upgrades_search_and_research_to_write_only`)
/// because they unconditionally enqueue background jobs — that dynamic
/// upgrade is orthogonal to the static class asserted here and is why their
/// rows were removed from the KNOWN_DIVERGENCE fixture above (resolved) while
/// `ask`/`evaluate`/`suggest`/`summarize` remain (no job-enqueuing trigger
/// exists for them yet).
#[test]
fn mcp_matches_tool_contract_read_only_query_surface_class() {
    for (action, subaction) in [
        ("query", ""),
        ("retrieve", ""),
        ("ask", ""),
        ("search", ""),
        ("research", ""),
        ("summarize", ""),
    ] {
        assert_eq!(
            mcp_scope_class(action, subaction),
            "read",
            "tool-contract.md's Auth and Visibility table classes `{action}` as axon:read"
        );
    }
}
