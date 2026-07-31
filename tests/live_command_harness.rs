#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn registry_mode_exercises_every_advertised_command_and_option() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("commands.json");
    fs::write(
        &registry,
        r#"{"commands":[
            {"name":"doctor","path":["doctor"],"mutates":false},
            {"name":"reset","path":["reset"],"mutates":true},
            {"name":"jobs list","path":["jobs","list"],"mutates":false}
        ]}"#,
    )
    .unwrap();

    let calls = temp.path().join("calls");
    let fake_axon = temp.path().join("axon");
    fs::write(
        &fake_axon,
        format!(
            "#!/usr/bin/env bash\n\
             printf '%s\\n' \"$*\" >> '{}'\n\
             printf 'Usage\\n  axon %s\\n' \"${{*% --help}}\"\n\
             printf 'Options\\n  --json  Machine output\\n  --limit <LIMIT>  Bound results\\n  -h, --help  Print help\\n'\n",
            calls.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_axon).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_axon, permissions).unwrap();

    let output = Command::new("bash")
        .arg("scripts/live-test-all-commands.sh")
        .args(["--mode", "registry"])
        .env("AXON_BIN", &fake_axon)
        .env("AXON_COMMAND_REGISTRY", &registry)
        .env("AXON_LIVE_TEST_OUTDIR", temp.path().join("results"))
        .output()
        .expect("run registry harness");

    assert!(
        output.status.success(),
        "harness failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = fs::read_to_string(calls).unwrap();
    let lines = calls.lines().collect::<Vec<_>>();
    for expected in [
        "--help",
        "--json --help",
        "--limit 1 --help",
        "doctor --help",
        "doctor --json --help",
        "reset --help",
        "jobs list --limit 1 --help",
        "--automation-script /tmp/axon-live-cli-contract --help",
        "--cache-http-only --help",
        "--viewport 1280x720 --help",
        "ask --continue --help",
        "completion --help",
        "setup hook --help",
    ] {
        assert!(
            lines.contains(&expected),
            "missing parser probe {expected:?} in:\n{calls}"
        );
    }
    assert_eq!(
        lines
            .iter()
            .filter(|line| **line == "--help" || line.ends_with(" --help"))
            .count(),
        lines.len(),
        "every parser probe must remain non-executing"
    );

    let report = fs::read_to_string(temp.path().join("results/report.tsv")).unwrap();
    assert!(
        report.lines().skip(1).all(|line| {
            let phase = line.split('\t').nth(1).unwrap_or_default();
            matches!(phase, "parser-command" | "parser-option" | "parser-alias")
        }),
        "registry mode must label non-executing checks as parser coverage:\n{report}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Parser checks:"),
        "registry summary must not describe parser-only checks as live CLI smoke"
    );
}

#[test]
fn scenario_mode_isolates_state_and_cleans_up_only_its_collection() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("commands.json");
    fs::write(
        &registry,
        r#"{"commands":[{"name":"config path","path":["config","path"],"mutates":false}]}"#,
    )
    .unwrap();

    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let calls = temp.path().join("calls");
    let fake_axon = bin_dir.join("axon");
    let fake_script = r#"#!/usr/bin/env bash
printf '%s|data=%s|sqlite=%s|server=%s|output=%s|artifact=%s|config=%s|envfile=%s\n' \
  "$*" "${AXON_DATA_DIR:-}" "${AXON_SQLITE_PATH:-}" "${AXON_SERVER_URL:-}" \
  "${AXON_OUTPUT_DIR:-}" "${AXON_ARTIFACT_BIN_DIR:-}" "${AXON_CONFIG_PATH:-}" \
  "${AXON_ENV_FILE:-}" >> '__CALLS__'
if [ "${1:-}" = setup ] && [ "${2:-}" = init ]; then
  mkdir -p "$HOME/.axon"
  : > "$HOME/.axon/.env"
fi
if [ "${1:-}" = config ] && [ "${2:-}" = path ]; then
  printf '{"toml_path":"%s","env_path":"%s"}\n' "$AXON_CONFIG_PATH" "$AXON_ENV_FILE"
else
  printf '{}\n'
fi
"#
    .replace("__CALLS__", &calls.display().to_string());
    fs::write(&fake_axon, fake_script).unwrap();
    let mut permissions = fs::metadata(&fake_axon).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_axon, permissions).unwrap();

    let curl_calls = temp.path().join("curl-calls");
    let fake_curl = bin_dir.join("curl");
    fs::write(
        &fake_curl,
        format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> '{}'\nprintf '{{}}\\n'\n",
            curl_calls.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_curl).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_curl, permissions).unwrap();

    let poison = temp.path().join("poison");
    let results = temp.path().join("results");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new("bash")
        .arg("scripts/live-test-all-commands.sh")
        .args(["--mode", "scenarios"])
        .env("PATH", path)
        .env("AXON_BIN", &fake_axon)
        .env("AXON_COMMAND_REGISTRY", &registry)
        .env("AXON_LIVE_TEST_OUTDIR", &results)
        .env("QDRANT_URL", "http://qdrant.live:6333")
        .env("TEI_URL", "http://tei.live:80")
        .env("AXON_DATA_DIR", poison.join("data"))
        .env("AXON_SQLITE_PATH", poison.join("jobs.db"))
        .env("AXON_SERVER_URL", "http://poison.invalid")
        .env("AXON_OUTPUT_DIR", poison.join("output"))
        .env("AXON_ARTIFACT_BIN_DIR", poison.join("artifacts"))
        .env("AXON_CONFIG_PATH", poison.join("config.toml"))
        .env("AXON_ENV_FILE", poison.join(".env"))
        .output()
        .expect("run isolated scenario harness");

    assert!(
        output.status.success(),
        "harness failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = fs::read_to_string(calls).unwrap();
    assert!(
        !calls.contains(&poison.display().to_string()) && !calls.contains("poison.invalid"),
        "poisoned caller state leaked into isolated commands:\n{calls}"
    );
    assert!(
        calls.lines().any(|line| {
            line.starts_with("config path --json|")
                && line.contains(&format!("data={}/data", results.display()))
                && line.contains("|sqlite=|server=|output=|artifact=|")
                && line.contains(&format!("config={}/config.toml", results.display()))
                && line.contains(&format!("envfile={}/.env", results.display()))
        }),
        "scenario did not receive the harness-owned state:\n{calls}"
    );

    let curl_calls = fs::read_to_string(curl_calls).unwrap();
    let cleanup_calls = curl_calls.lines().collect::<Vec<_>>();
    assert_eq!(
        cleanup_calls.len(),
        1,
        "cleanup must issue exactly one collection deletion:\n{curl_calls}"
    );
    assert!(
        cleanup_calls[0]
            .starts_with("-fsS -X DELETE http://qdrant.live:6333/collections/axon_live_"),
        "cleanup must target only the generated isolated collection:\n{curl_calls}"
    );
}
