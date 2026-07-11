//! WS-H / audit D1 (issue #298): cross-surface CLI<->REST<->MCP parity
//! matrix.
//!
//! Builds on `tests/http_api_parity_inventory.rs` and
//! `tests/mcp_contract_parity.rs`, which check REST<->OpenAPI and MCP schema
//! shapes in isolation. This file instead asks one question per contracted
//! operation: is the operation present (or intentionally/documented-ly
//! absent) *consistently* across all three transports — the live CLI command
//! tree (`axon --help`), the REST route inventory
//! (`axon_services::types::rest_route_inventory`), and the MCP action
//! registry (`axon_mcp::server::required_scope_for`)?
//!
//! The expected matrix lives in
//! `tests/fixtures/cross-surface/operation_matrix.json` so drift shows up as
//! a fixture diff, not a buried assertion string. Every row's expected
//! booleans are checked against live system state — a row is never skipped,
//! including documented `divergence: true` rows, so an accidental *fix* of a
//! known gap (or a new regression) both fail loudly until the fixture is
//! updated by whoever reviews the change.

use axon_mcp::server::required_scope_for;
use axon_services::types::rest_route_inventory;
use serde::Deserialize;
use std::collections::HashSet;
use std::process::Command;
use std::sync::LazyLock;

const FIXTURE: &str = include_str!("fixtures/cross-surface/operation_matrix.json");

#[derive(Debug, Deserialize)]
struct OperationMatrixFixture {
    operations: Vec<OperationRow>,
}

#[derive(Debug, Deserialize)]
struct OperationRow {
    op: String,
    cli: bool,
    rest: bool,
    mcp: bool,
    cli_command: Option<String>,
    rest_paths: Vec<String>,
    mcp_action: Option<String>,
    divergence: bool,
    note: String,
}

/// Top-level CLI subcommand names, parsed once from the live `axon --help`
/// output (4-space-indented `name<gap>description` lines under the
/// `Commands` heading; 2-space-indented lines are section headers, not
/// commands). Parsing the real binary output (rather than hand-copying a
/// command list) means an added/removed CLI command changes this set
/// automatically.
static CLI_COMMANDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    let output = Command::new(env!("CARGO_BIN_EXE_axon"))
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to execute axon binary for --help");
    assert!(
        output.status.success(),
        "axon --help failed: status={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("    ")?;
            // Section headers are 2-space indented; anything reaching here
            // had exactly-or-more 4-space indent. Reject lines that are
            // *further* indented flag/description continuations by requiring
            // the first char to start a lowercase command token immediately
            // (no extra leading space) and the line to contain the
            // multi-space gap before a description.
            if rest.starts_with(' ') {
                return None;
            }
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '-')
                .collect();
            if name.is_empty() {
                return None;
            }
            // Guard against accidentally matching a global-options line
            // (e.g. "--wait <bool>") — command names never start with '-'.
            if !rest.starts_with(name.chars().next().unwrap()) {
                return None;
            }
            Some(name)
        })
        .collect()
});

fn cli_has_command(name: &str) -> bool {
    CLI_COMMANDS.contains(name)
}

fn rest_has_any_path(paths: &[String]) -> bool {
    let inventory = rest_route_inventory();
    paths
        .iter()
        .any(|path| inventory.iter().any(|route| route.path == path))
}

fn mcp_has_action(action: &str) -> bool {
    // required_scope_for returns Some("__deny__") for any action name absent
    // from MCP_ACTION_SPECS (and the few hand-special-cased actions), and
    // `Some(<real scope>)`/`None` (InfoOnly) for recognized ones. This lets
    // us probe action-name membership without needing a public accessor for
    // the private MCP_ACTION_SPECS table. A few actions (`jobs`, `memory`,
    // `watch`) resolve scope entirely from `subaction` and reject the empty
    // string — probe with a subaction that is known-valid for each so
    // membership doesn't false-negative on those.
    let probe_subaction = match action {
        "jobs" | "watch" => "list",
        "memory" => "search",
        _ => "",
    };
    required_scope_for(action, probe_subaction) != Some("__deny__")
}

#[test]
fn operation_matrix_fixture_is_not_vacuous() {
    let fixture: OperationMatrixFixture =
        serde_json::from_str(FIXTURE).expect("parse operation_matrix.json fixture");
    assert!(
        fixture.operations.len() >= 10,
        "operation matrix fixture looks too small to be meaningful"
    );
    let fully_consistent = fixture
        .operations
        .iter()
        .filter(|row| !row.divergence && row.cli && row.rest && row.mcp)
        .count();
    let documented_divergences = fixture
        .operations
        .iter()
        .filter(|row| row.divergence)
        .count();
    assert!(
        fully_consistent >= 5,
        "expected several operations present on all three surfaces; matrix may be miscoded"
    );
    assert!(
        documented_divergences >= 3,
        "expected at least a few KNOWN_DIVERGENCES rows; matrix may be miscoded"
    );
}

#[test]
fn cli_rest_mcp_presence_matches_fixture_for_every_operation() {
    let fixture: OperationMatrixFixture =
        serde_json::from_str(FIXTURE).expect("parse operation_matrix.json fixture");

    for row in &fixture.operations {
        let actual_cli = row
            .cli_command
            .as_deref()
            .map(cli_has_command)
            .unwrap_or(false);
        let actual_rest = rest_has_any_path(&row.rest_paths);
        let actual_mcp = row
            .mcp_action
            .as_deref()
            .map(mcp_has_action)
            .unwrap_or(false);

        assert_eq!(
            actual_cli, row.cli,
            "operation `{}`: CLI presence drifted from fixture (expected {}, got {}). \
             cli_command={:?}. {}",
            row.op, row.cli, actual_cli, row.cli_command, row.note
        );
        assert_eq!(
            actual_rest, row.rest,
            "operation `{}`: REST presence drifted from fixture (expected {}, got {}). \
             rest_paths={:?}. {}",
            row.op, row.rest, actual_rest, row.rest_paths, row.note
        );
        assert_eq!(
            actual_mcp, row.mcp,
            "operation `{}`: MCP presence drifted from fixture (expected {}, got {}). \
             mcp_action={:?}. {}",
            row.op, row.mcp, actual_mcp, row.mcp_action, row.note
        );

        // A row's own internal consistency: any row where the three surfaces
        // disagree (a genuine gap) or that is otherwise flagged `divergence`
        // must carry a KNOWN_DIVERGENCE note explaining/tracking it — this
        // catches fixture authoring mistakes, not just runtime drift.
        // `resolve` is the one carve-out: cli=false/rest=true/mcp=true is an
        // *intentional*, doc-stated absence ("internal/diagnostic" in
        // tool-contract.md), not an unexplained gap, so it is allowed to sit
        // at `divergence: false` as long as its note says so explicitly.
        let all_agree = row.cli == row.rest && row.rest == row.mcp;
        if all_agree {
            assert!(
                !row.divergence,
                "operation `{}`: all three surfaces agree ({}, {}, {}) but the row is \
                 flagged `divergence: true` — fix the fixture row itself",
                row.op, row.cli, row.rest, row.mcp
            );
        } else {
            assert!(
                !row.note.is_empty(),
                "operation `{}`: cli/rest/mcp booleans disagree ({}, {}, {}) but the row has \
                 no note explaining whether this is intentional or a KNOWN_DIVERGENCE",
                row.op,
                row.cli,
                row.rest,
                row.mcp
            );
        }
        if row.divergence {
            assert!(
                row.note.contains("KNOWN_DIVERGENCE"),
                "operation `{}` is flagged divergence but has no KNOWN_DIVERGENCE note \
                 explaining/tracking it",
                row.op
            );
        }
    }
}

/// Sanity guard: every fixture `cli_command` that's non-null must actually be
/// null exactly when `cli` is false, and non-null when `cli` is true — a
/// stray `cli_command` on a `cli: false` row (or the reverse) would silently
/// make the matrix vacuous for that surface.
#[test]
fn fixture_cli_command_presence_matches_cli_flag() {
    let fixture: OperationMatrixFixture =
        serde_json::from_str(FIXTURE).expect("parse operation_matrix.json fixture");
    for row in &fixture.operations {
        assert_eq!(
            row.cli_command.is_some(),
            row.cli,
            "operation `{}`: cli_command presence must match `cli` boolean",
            row.op
        );
        assert_eq!(
            row.mcp_action.is_some(),
            row.mcp,
            "operation `{}`: mcp_action presence must match `mcp` boolean",
            row.op
        );
        assert!(
            !row.rest_paths.is_empty(),
            "operation `{}`: rest_paths must never be empty (use a representative path even \
             when `rest` is false, so the negative check is meaningful)",
            row.op
        );
    }
}
