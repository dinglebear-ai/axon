//! Golden-file coverage for representative `axon <cmd> --json` envelopes
//! (D1-10, testing-contract.md "Golden Files": "CLI `--json` envelopes").
//!
//! Each test runs the freshly built `axon` binary against an isolated,
//! throwaway `HOME`/`AXON_DATA_DIR` so output is deterministic and does not
//! depend on the operator's real `~/.axon` state. The only non-deterministic
//! substring is the tempdir path itself, which is normalized to `<TMPDIR>`
//! before comparing against the checked-in fixture.
//!
//! To intentionally update a fixture after reviewing a real contract change,
//! run the command manually (see the `run_json` call in each test for the
//! exact args/env) and copy its stdout into the matching
//! `tests/fixtures/cli-json/*.json` file, then re-normalize the tempdir path
//! to `<TMPDIR>`.

use std::path::Path;
use std::process::Command;

fn json_fixture(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/cli-json")
            .join(name),
    )
    .expect("failed to read cli-json fixture")
}

/// Runs the built `axon` binary against an isolated temp home, returning
/// stdout with the tempdir path normalized to `<TMPDIR>`.
fn run_json_isolated(args: &[&str], extra_envs: &[(&str, &str)]) -> String {
    let tmp = tempfile::tempdir().expect("failed to create temp home");
    let home = tmp.path();
    let data_dir = home.join(".axon");
    std::fs::create_dir_all(&data_dir).expect("failed to create data dir");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_axon"));
    cmd.args(args)
        .env("NO_COLOR", "1")
        .env("HOME", home)
        .env("AXON_DATA_DIR", &data_dir);
    for (key, value) in extra_envs {
        cmd.env(key, value);
    }

    // Warm-up run: the first invocation against a fresh data dir emits a
    // one-time "tightened directory permissions" WARN on stderr (not
    // stdout), and creates jobs.db. Run once to settle that before the
    // measured run, keeping the measured run's stdout free of ordering
    // noise between the warning and the JSON payload.
    let _ = cmd.output();

    let output = Command::new(env!("CARGO_BIN_EXE_axon"))
        .args(args)
        .env("NO_COLOR", "1")
        .env("HOME", home)
        .env("AXON_DATA_DIR", &data_dir)
        .envs(extra_envs.iter().copied())
        .output()
        .expect("failed to execute axon binary");
    assert!(
        output.status.success(),
        "axon command failed: args={args:?} status={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let home_str = home.to_string_lossy();
    stdout.replace(home_str.as_ref(), "<TMPDIR>")
}

fn assert_json_snapshot(args: &[&str], extra_envs: &[(&str, &str)], fixture: &str) {
    let actual = run_json_isolated(args, extra_envs);
    let expected = json_fixture(fixture);
    let actual_value: serde_json::Value = serde_json::from_str(actual.trim()).unwrap_or_else(|e| {
        panic!("actual stdout for args={args:?} is not valid JSON: {e}\nstdout={actual}")
    });
    let expected_value: serde_json::Value = serde_json::from_str(&expected)
        .unwrap_or_else(|e| panic!("fixture {fixture} is not valid JSON: {e}"));
    assert_eq!(
        actual_value, expected_value,
        "cli-json snapshot drift for args={args:?}; update {fixture} only after reviewing the \
         CLI output (see module docs for the manual regen steps)"
    );
}

#[test]
fn status_json_envelope_matches_golden_fixture() {
    assert_json_snapshot(
        &["status", "--json"],
        &[
            ("TEI_URL", "http://127.0.0.1:1"),
            ("QDRANT_URL", "http://127.0.0.1:2"),
            ("AXON_LLM_BACKEND", "gemini-headless"),
        ],
        "status.json",
    );
}

#[test]
fn config_get_missing_key_json_envelope_matches_golden_fixture() {
    assert_json_snapshot(
        &["config", "get", "NOT_A_REAL_KEY", "--json"],
        &[],
        "config-get-missing-key.json",
    );
}
