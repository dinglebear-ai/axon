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
fn worktree_fingerprint_batches_hashing_and_detects_mutations() {
    let helper_path =
        fs::canonicalize("scripts/lib/live-cli-reporting.sh").expect("canonical reporting helper");
    let helper = fs::read_to_string(&helper_path).expect("read reporting helper");
    assert!(
        helper.contains("xargs -0 -r sha256sum -z --"),
        "regular files must be hashed in batches"
    );
    assert!(
        !helper.contains("sha256sum -- \"$path\""),
        "the fingerprint must not spawn one sha256sum process per file"
    );

    let temp = tempfile::tempdir().expect("tempdir");
    let git_status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(temp.path())
        .status()
        .expect("initialize temporary repository");
    assert!(git_status.success());
    fs::write(temp.path().join("alpha.txt"), "alpha\n").unwrap();
    fs::write(temp.path().join("beta.txt"), "beta\n").unwrap();
    std::os::unix::fs::symlink("alpha.txt", temp.path().join("current")).unwrap();

    let shell = r#"
set -euo pipefail
ROOT_DIR="$FINGERPRINT_ROOT"
OUTDIR="$ROOT_DIR/out"
SETUP_HOME="$ROOT_DIR/setup-home"
isolated_compose_project=""
isolated_compose_network=""
isolated_collections=()
QDRANT_URL=""
source "$FINGERPRINT_HELPER"
before="$(worktree_content_fingerprint)"
same="$(worktree_content_fingerprint)"
printf 'changed\n' > "$ROOT_DIR/alpha.txt"
after_file="$(worktree_content_fingerprint)"
rm -- "$ROOT_DIR/current"
ln -s beta.txt "$ROOT_DIR/current"
after_link="$(worktree_content_fingerprint)"
printf '%s\n%s\n%s\n%s\n' "$before" "$same" "$after_file" "$after_link"
"#;
    let output = Command::new("bash")
        .arg("-c")
        .arg(shell)
        .env("FINGERPRINT_ROOT", temp.path())
        .env("FINGERPRINT_HELPER", helper_path)
        .output()
        .expect("run worktree fingerprint probe");
    assert!(
        output.status.success(),
        "fingerprint probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let hashes = String::from_utf8(output.stdout)
        .expect("fingerprint output")
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(hashes.len(), 4, "expected four fingerprint samples");
    assert!(hashes.iter().all(|hash| hash.len() == 64));
    assert_eq!(hashes[0], hashes[1], "unchanged content must be stable");
    assert_ne!(
        hashes[1], hashes[2],
        "file content changes must be detected"
    );
    assert_ne!(
        hashes[2], hashes[3],
        "symlink target changes must be detected"
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
printf '%s|cwd=%s|data=%s|sqlite=%s|server=%s|output=%s|artifact=%s|config=%s|envfile=%s|chrome=%s\n' \
  "$*" "$PWD" "${AXON_DATA_DIR:-}" "${AXON_SQLITE_PATH:-}" "${AXON_SERVER_URL:-}" \
  "${AXON_OUTPUT_DIR:-}" "${AXON_ARTIFACT_BIN_DIR:-}" "${AXON_CONFIG_PATH:-}" \
  "${AXON_ENV_FILE:-}" "${AXON_CHROME_REMOTE_URL:-}" >> '__CALLS__'
if [ "${1:-}" = --help ]; then
  printf '  AXON CLI

  Global Options
  --json  Machine output

  Commands
'
  exit 0
fi
if [ "${1:-}" = --json ]; then
  shift
fi
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
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> '{}'\nif [[ \"$*\" == *'/json/version'* && \"$*\" != *':9222/json/version'* ]]; then exit 7; fi\nprintf '{{}}\\n'\n",
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
    let command_workdir = format!("cwd={}/command-workdir", results.display());
    assert!(
        calls.lines().all(|line| line.contains(&command_workdir)),
        "all live commands must run from the harness-owned working directory:\n{calls}"
    );
    assert!(
        calls.lines().any(|line| {
            line.starts_with("--json config path|")
                && line.contains(&format!("data={}/data", results.display()))
                && line.contains("|sqlite=|server=|output=|artifact=|")
                && line.contains(&format!("config={}/config.toml", results.display()))
                && line.contains(&format!("envfile={}/.env", results.display()))
        }),
        "scenario did not receive the harness-owned state:\n{calls}"
    );
    assert!(
        calls
            .lines()
            .all(|line| line.contains("|chrome=http://127.0.0.1:9222")),
        "setup and runtime commands must use the same reachable live Chrome endpoint:\n{calls}"
    );

    let behavior_actual = fs::read_to_string(results.join("behavioral-actual.tsv")).unwrap();
    assert!(
        behavior_actual.lines().any(|line| line == "@global	--json"),
        "global --json usage must be recorded under @global:
{behavior_actual}"
    );
    assert!(
        !behavior_actual
            .lines()
            .any(|line| line == "config path	--json"),
        "global options must not inflate command-local evidence:
{behavior_actual}"
    );

    let curl_calls = fs::read_to_string(curl_calls).unwrap();
    let cleanup_calls = curl_calls
        .lines()
        .filter(|line| line.contains("-X DELETE"))
        .collect::<Vec<_>>();
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

#[test]
fn scenario_mode_rejects_explicit_data_dir_outside_harness_tree() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("commands.json");
    fs::write(
        &registry,
        r#"{"commands":[{"name":"config path","path":["config","path"],"mutates":false}]}"#,
    )
    .unwrap();
    let output = Command::new("bash")
        .arg("scripts/live-test-all-commands.sh")
        .args(["--mode", "scenarios"])
        .env("AXON_BIN", "/bin/true")
        .env("AXON_COMMAND_REGISTRY", &registry)
        .env("AXON_LIVE_TEST_OUTDIR", temp.path().join("results"))
        .env("AXON_LIVE_DATA_DIR", temp.path().join("production-state"))
        .output()
        .expect("run destructive-state guard");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("AXON_LIVE_DATA_DIR must remain inside the harness output tree"),
        "missing destructive-state refusal: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn canonical_binary_rejects_invalid_values_and_conflicts_without_help() {
    let binary = env!("CARGO_BIN_EXE_axon");
    for (args, expected) in [
        (
            vec![
                "source",
                "https://example.com",
                "--render-mode",
                "bogus",
                "--json",
            ],
            "invalid value 'bogus'",
        ),
        (
            vec!["status", "--active", "--recent", "--json"],
            "cannot be used with '--recent'",
        ),
    ] {
        let output = Command::new(binary)
            .args(&args)
            .output()
            .expect("run canonical parser rejection");
        assert_eq!(
            output.status.code(),
            Some(2),
            "unexpected parser exit for {args:?}:
stdout={}
stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "missing parser error {expected:?} for {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("Usage: axon --help"),
            "parser rejection must identify the command-specific failure"
        );
    }
}

#[test]
fn cleanup_deletes_every_generated_collection_and_ignores_foreign_names() {
    let temp = tempfile::tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let calls = temp.path().join("curl-calls");
    let fake_curl = bin_dir.join("curl");
    fs::write(
        &fake_curl,
        format!(
            r#"#!/usr/bin/env bash
printf '%s
' "$*" >> '{}'
printf '{{}}
'
"#,
            calls.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_curl).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_curl, permissions).unwrap();

    let outdir = temp.path().join("results");
    fs::create_dir_all(outdir.join("logs")).unwrap();
    let shell = format!(
        r#"set -u
OUTDIR='{}'
SETUP_HOME='{}'
isolated_compose_project=''
isolated_compose_network=''
isolated_collections=(axon_live_one foreign_collection axon_live_two)
QDRANT_URL='http://qdrant.live:6333'
source scripts/lib/live-cli-reporting.sh
"#,
        outdir.display(),
        temp.path().join("setup-home").display()
    );
    let path_env = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new("bash")
        .arg("-c")
        .arg(shell)
        .env("PATH", path_env)
        .output()
        .expect("run cleanup trap");
    assert!(
        output.status.success(),
        "cleanup trap failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let calls = fs::read_to_string(calls).unwrap();
    let lines = calls.lines().collect::<Vec<_>>();
    assert_eq!(
        lines.len(),
        2,
        "expected two generated deletions:
{calls}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.ends_with("/collections/axon_live_one"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.ends_with("/collections/axon_live_two"))
    );
    assert!(
        !calls.contains("foreign_collection"),
        "foreign collection must never be deleted:
{calls}"
    );
}

#[test]
fn hidden_cli_alias_inventory_is_behaviorally_covered() {
    let cli = fs::read_to_string("crates/axon-core/src/config/cli.rs").expect("read CLI schema");
    assert!(
        !cli.contains("aliases =")
            && !cli.contains("visible_alias")
            && !cli.contains("short_alias"),
        "extend the alias inventory parser when a new alias declaration form is introduced"
    );

    let marker = "alias = \"";
    let mut remaining = cli.as_str();
    let mut aliases = Vec::new();
    while let Some(start) = remaining.find(marker) {
        remaining = &remaining[start + marker.len()..];
        let end = remaining.find('\"').expect("terminated CLI alias");
        aliases.push(remaining[..end].to_string());
        remaining = &remaining[end + 1..];
    }
    aliases.sort();
    assert_eq!(aliases, ["completion", "continue", "hook"]);

    let completion = fs::read_to_string("scripts/lib/live-cli-scenarios-jobs-source.sh")
        .expect("read completion scenarios");
    assert!(completion.contains("completion alias equivalence"));

    let setup = fs::read_to_string("scripts/lib/live-cli-scenarios-admin.sh")
        .expect("read setup scenarios");
    assert!(setup.contains("setup hook alias equivalence"));

    let ask = fs::read_to_string("scripts/lib/live-cli-scenarios-web-rag.sh")
        .expect("read ask scenarios");
    assert!(ask.contains("prove_option_behavior \"ask\" \"--continue\""));
}
