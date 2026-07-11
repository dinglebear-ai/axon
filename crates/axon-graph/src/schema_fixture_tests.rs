//! Validates the graph-schema.md "Validation Fixtures" set
//! (`crates/axon-graph/tests/fixtures/schema/*.json`) against
//! [`crate::candidate::validate_candidate`].

use axon_api::source::GraphCandidate;

use crate::candidate::validate_candidate;

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/schema")
        .join(name)
}

fn load(name: &str) -> GraphCandidate {
    let raw = std::fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|err| panic!("read fixture {name}: {err}"));
    serde_json::from_str(&raw).unwrap_or_else(|err| panic!("parse fixture {name}: {err}"))
}

#[test]
fn repo_package_fixture_validates() {
    validate_candidate(&load("repo_package.valid.json")).expect("repo_package fixture is valid");
}

#[test]
fn code_symbol_fixture_validates() {
    validate_candidate(&load("code_symbol.valid.json")).expect("code_symbol fixture is valid");
}

#[test]
fn session_tool_skill_fixture_validates() {
    validate_candidate(&load("session_tool_skill.valid.json"))
        .expect("session_tool_skill fixture is valid");
}

#[test]
fn docker_compose_fixture_validates() {
    validate_candidate(&load("docker_compose.valid.json"))
        .expect("docker_compose fixture is valid");
}

#[test]
fn unknown_kind_fixture_is_rejected() {
    let err = validate_candidate(&load("unknown_kind.invalid.json"))
        .expect_err("unknown node kind must be rejected");
    assert!(
        err.message.contains("unknown graph node kind"),
        "{}",
        err.message
    );
}

#[test]
fn missing_evidence_fixture_is_rejected() {
    let err = validate_candidate(&load("missing_evidence.invalid.json"))
        .expect_err("edge without evidence must be rejected");
    assert!(err.message.contains("no evidence"), "{}", err.message);
}
