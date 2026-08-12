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
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> '{}'\nif [[ \"$*\" == *'/json/version'* && \"$*\" != *':9222/json/version'* ]]; then exit 7; fi\nif [[ \"$*\" == *':9222/json/version'* ]]; then printf '{{\"Browser\":\"HeadlessChrome/1\",\"webSocketDebuggerUrl\":\"ws://127.0.0.1:9222/devtools/browser/test\"}}\\n'; else printf '{{}}\\n'; fi\n",
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
        cleanup_calls[0].contains("http://qdrant.live:6333/collections/axon_live_"),
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
fn scenario_mode_rejects_an_explicit_non_cdp_chrome_endpoint() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("commands.json");
    fs::write(&registry, r#"{"commands":[]}"#).unwrap();

    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let fake_axon = bin_dir.join("axon");
    fs::write(
        &fake_axon,
        "#!/usr/bin/env bash\nif [ \"${1:-}\" = setup ]; then mkdir -p \"$HOME/.axon\"; : > \"$HOME/.axon/.env\"; fi\nprintf '{}\\n'\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_axon).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_axon, permissions).unwrap();

    let fake_curl = bin_dir.join("curl");
    fs::write(
        &fake_curl,
        "#!/usr/bin/env bash\nprintf '<html>not chrome</html>\\n'\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_curl).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_curl, permissions).unwrap();

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
        .env("AXON_LIVE_TEST_OUTDIR", temp.path().join("results"))
        .env("AXON_LIVE_CHROME_REMOTE_URL", "http://127.0.0.1:45555")
        .output()
        .expect("run scenario harness with invalid explicit Chrome endpoint");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("explicit Chrome endpoint is not a reachable CDP endpoint")
            && stderr.contains("http://127.0.0.1:45555"),
        "missing targeted Chrome endpoint diagnostic: {stderr}"
    );
}

#[test]
fn scenario_mode_owns_a_collision_safe_chrome_and_bounds_cleanup() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("commands.json");
    fs::write(&registry, r#"{"commands":[]}"#).unwrap();

    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let fake_axon = bin_dir.join("axon");
    fs::write(
        &fake_axon,
        "#!/usr/bin/env bash\nif [ \"${1:-}\" = setup ]; then mkdir -p \"$HOME/.axon\"; : > \"$HOME/.axon/.env\"; fi\nprintf '{}\\n'\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_axon).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_axon, permissions).unwrap();

    let chrome_args = temp.path().join("chrome-args");
    let chrome_pid = temp.path().join("chrome-pid");
    let chrome_child_pid = temp.path().join("chrome-child-pid");
    let devtools_port = temp.path().join("devtools-port");
    let fake_chrome = bin_dir.join("google-chrome");
    let chrome_script = format!(
        r#"#!/usr/bin/env bash
printf '%s\n' "$*" > '{}'
printf '%s\n' "$$" > '{}'
(trap '' TERM; while :; do sleep 1; done) &
printf '%s\n' "$!" > '{}'
profile=""
for arg in "$@"; do
  case "$arg" in --user-data-dir=*) profile="${{arg#*=}}" ;; esac
done
mkdir -p "$profile"
printf '45678\n/devtools/browser/owned\n' > "$profile/DevToolsActivePort"
printf '45678\n' > '{}'
trap '' TERM
while :; do read -r -t 1 _unused || true; done
"#,
        chrome_args.display(),
        chrome_pid.display(),
        chrome_child_pid.display(),
        devtools_port.display()
    );
    fs::write(&fake_chrome, chrome_script).unwrap();
    let mut permissions = fs::metadata(&fake_chrome).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_chrome, permissions).unwrap();

    let fake_curl = bin_dir.join("curl");
    let curl_script = format!(
        r#"#!/usr/bin/env bash
url="${{@: -1}}"
if [[ "$url" == *':45678/json/version' ]] && [ -s '{}' ]; then
  printf '{{"Browser":"HeadlessChrome/1","webSocketDebuggerUrl":"ws://127.0.0.1:45678/devtools/browser/owned"}}\n'
  exit 0
fi
[[ "$*" == *' -X DELETE '* ]] && {{ printf '{{}}\n'; exit 0; }}
exit 7
"#,
        devtools_port.display()
    );
    fs::write(&fake_curl, curl_script).unwrap();
    let mut permissions = fs::metadata(&fake_curl).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_curl, permissions).unwrap();

    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new("timeout")
        .args([
            "--kill-after=2",
            "30",
            "bash",
            "scripts/live-test-all-commands.sh",
            "--mode",
            "scenarios",
        ])
        .env("PATH", path)
        .env("AXON_BIN", &fake_axon)
        .env("AXON_COMMAND_REGISTRY", &registry)
        .env("AXON_LIVE_TEST_OUTDIR", temp.path().join("results"))
        .output()
        .expect("run scenario harness with owned Chrome");

    assert!(
        output.status.success(),
        "owned Chrome harness failed or cleanup timed out: status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let args = fs::read_to_string(chrome_args).expect("owned Chrome arguments");
    assert!(
        args.contains("--remote-debugging-port=0"),
        "default owned Chrome must ask the OS for a collision-safe port: {args}"
    );
    let pid = fs::read_to_string(chrome_pid).expect("owned Chrome pid");
    let status = Command::new("kill")
        .args(["-0", pid.trim()])
        .status()
        .expect("probe owned Chrome pid");
    assert!(
        !status.success(),
        "owned Chrome process survived harness cleanup"
    );
    let child_pid = fs::read_to_string(chrome_child_pid).expect("owned Chrome child pid");
    let child_status = Command::new("kill")
        .args(["-0", child_pid.trim()])
        .status()
        .expect("probe owned Chrome child pid");
    assert!(
        !child_status.success(),
        "owned Chrome descendant survived harness cleanup"
    );
}

#[test]
fn scenario_mode_refuses_a_foreign_cdp_on_an_explicit_fixed_port() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("commands.json");
    fs::write(&registry, r#"{"commands":[]}"#).unwrap();
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let fake_axon = bin_dir.join("axon");
    fs::write(&fake_axon, "#!/usr/bin/env bash\nprintf '{}\\n'\n").unwrap();
    let fake_chrome = bin_dir.join("google-chrome");
    fs::write(&fake_chrome, "#!/usr/bin/env bash\nexit 99\n").unwrap();
    let fake_curl = bin_dir.join("curl");
    fs::write(
        &fake_curl,
        "#!/usr/bin/env bash\nif [[ \"$*\" == *':45678/json/version'* ]]; then printf '{\"Browser\":\"HeadlessChrome/1\",\"webSocketDebuggerUrl\":\"ws://127.0.0.1:45678/devtools/browser/foreign\"}\\n'; exit 0; fi\nexit 7\n",
    )
    .unwrap();
    for path in [&fake_axon, &fake_chrome, &fake_curl] {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap());
    let output = Command::new("bash")
        .arg("scripts/live-test-all-commands.sh")
        .args(["--mode", "scenarios"])
        .env("PATH", path)
        .env("AXON_BIN", &fake_axon)
        .env("AXON_COMMAND_REGISTRY", &registry)
        .env("AXON_LIVE_TEST_OUTDIR", temp.path().join("results"))
        .env("AXON_LIVE_CHROME_PORT", "45678")
        .output()
        .expect("run scenario harness with occupied fixed Chrome port");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("refusing foreign CDP endpoint on fixed port 45678"),
        "missing foreign fixed-port diagnostic: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn scenario_mode_rejects_a_devtools_port_file_bound_to_another_endpoint() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("commands.json");
    fs::write(&registry, r#"{"commands":[]}"#).unwrap();
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let fake_axon = bin_dir.join("axon");
    fs::write(&fake_axon, "#!/usr/bin/env bash\nprintf '{}\\n'\n").unwrap();
    let fake_chrome = bin_dir.join("google-chrome");
    fs::write(
        &fake_chrome,
        "#!/usr/bin/env bash\nprofile=\"\"\nfor arg in \"$@\"; do case \"$arg\" in --user-data-dir=*) profile=\"${arg#*=}\" ;; esac; done\nmkdir -p \"$profile\"\nprintf '45678\\n/devtools/browser/owned\\n' > \"$profile/DevToolsActivePort\"\ntrap '' TERM\nwhile :; do sleep 1; done\n",
    )
    .unwrap();
    let fake_curl = bin_dir.join("curl");
    fs::write(
        &fake_curl,
        "#!/usr/bin/env bash\nif [[ \"$*\" == *':45678/json/version'* ]]; then printf '{\"Browser\":\"HeadlessChrome/1\",\"webSocketDebuggerUrl\":\"ws://127.0.0.1:49999/devtools/browser/foreign\"}\\n'; exit 0; fi\nexit 7\n",
    )
    .unwrap();
    for path in [&fake_axon, &fake_chrome, &fake_curl] {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap());
    let output = Command::new("bash")
        .arg("scripts/live-test-all-commands.sh")
        .args(["--mode", "scenarios"])
        .env("PATH", path)
        .env("AXON_BIN", &fake_axon)
        .env("AXON_COMMAND_REGISTRY", &registry)
        .env("AXON_LIVE_TEST_OUTDIR", temp.path().join("results"))
        .output()
        .expect("run scenario harness with mismatched DevTools endpoint");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("harness-owned Chrome did not become ready")
    );
}

#[test]
fn artifact_scenarios_seed_their_own_fixture_when_screenshot_is_unavailable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("commands.json");
    fs::write(
        &registry,
        r#"{"commands":[
            {"name":"artifacts list","path":["artifacts","list"],"mutates":false},
            {"name":"artifacts get","path":["artifacts","get"],"mutates":false},
            {"name":"artifacts content","path":["artifacts","content"],"mutates":false}
        ]}"#,
    )
    .unwrap();

    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let calls = temp.path().join("calls");
    let sequence = temp.path().join("sequence");
    let fake_axon = bin_dir.join("axon");
    let fake_script = r#"#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> '__CALLS__'
args=("$@")
filtered=()
for arg in "${args[@]}"; do
  case "$arg" in --json|--quiet) ;; *) filtered+=("$arg") ;; esac
done
set -- "${filtered[@]}"
if [ "${1:-}" = config ]; then printf '{}\n'; exit 0; fi
if [ "${1:-}" = setup ] && [ "${2:-}" = init ]; then
  mkdir -p "$HOME/.axon"
  : > "$HOME/.axon/.env"
  printf '{}\n'
  exit 0
fi
if [ "${1:-}" = screenshot ]; then exit 1; fi
if [ "${1:-}" = uploads ] && [ "${2:-}" = create ]; then
  count=0
  [ ! -f '__SEQUENCE__' ] || count="$(cat '__SEQUENCE__')"
  count=$((count + 1))
  printf '%s' "$count" > '__SEQUENCE__'
  printf '{"upload":{"upload_id":"upl_fixture_%s"},"status":{"upload_id":"upl_fixture_%s"}}\n' "$count" "$count"
  exit 0
fi
if [ "${1:-}" = uploads ] && [ "${2:-}" = complete ]; then
  suffix="${3##*_}"
  printf '{"upload_id":"%s","artifact_id":"art_raw_fixture_%s"}\n' "$3" "$suffix"
  exit 0
fi
if [ "${1:-}" = artifacts ] && [ "${2:-}" = list ]; then
  if [[ " $* " == *" --source-id "* || " $* " == *" --job-id "* ]]; then
    printf '{"items":[],"next_cursor":null,"limit":10}\n'
  elif [[ " $* " == *" --cursor "* ]]; then
    printf '{"items":[{"artifact_id":"art_raw_fixture_2"}],"next_cursor":null,"limit":1}\n'
  else
    printf '{"items":[{"artifact_id":"art_raw_fixture_1"}],"next_cursor":"art_raw_fixture_1","limit":1}\n'
  fi
  exit 0
fi
if [ "${1:-}" = artifacts ] && [ "${2:-}" = get ]; then
  [ -n "${3:-}" ]
  printf '{"artifact_id":"%s","content_url":"/v1/artifacts/%s/content","metadata":{"filename":"hosts"}}\n' "$3" "$3"
  exit 0
fi
if [ "${1:-}" = artifacts ] && [ "${2:-}" = content ]; then
  [ -n "${3:-}" ]
  output=""
  range=""
  download=0
  index=4
  while [ "$index" -le "$#" ]; do
    value="${!index}"
    case "$value" in
      --output) index=$((index + 1)); output="${!index}" ;;
      --range) index=$((index + 1)); range="${!index}" ;;
      --download) download=1 ;;
    esac
    index=$((index + 1))
  done
  [ "$download" -eq 0 ] || output="$3.bin"
  size=32
  [ "$range" != bytes=0-15 ] || size=16
  [ -z "$output" ] || head -c "$size" /dev/zero > "$output"
  printf '{"artifact_id":"%s","content_type":"application/octet-stream","size_bytes":%s,"output":"%s"}\n' "$3" "$size" "$output"
  exit 0
fi
printf '{}\n'
"#
    .replace("__CALLS__", &calls.display().to_string())
    .replace("__SEQUENCE__", &sequence.display().to_string());
    fs::write(&fake_axon, fake_script).unwrap();
    let mut permissions = fs::metadata(&fake_axon).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_axon, permissions).unwrap();

    let fake_curl = bin_dir.join("curl");
    fs::write(
        &fake_curl,
        "#!/usr/bin/env bash\nif [[ \"$*\" == *':9222/json/version'* ]]; then printf '{\"Browser\":\"HeadlessChrome/1\",\"webSocketDebuggerUrl\":\"ws://127.0.0.1:9222/devtools/browser/test\"}\\n'; else printf '{}\\n'; fi\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_curl).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_curl, permissions).unwrap();

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
        .output()
        .expect("run artifact scenarios");

    assert!(
        output.status.success(),
        "artifact scenarios failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = fs::read_to_string(calls).unwrap();
    assert!(
        calls.contains("uploads complete upl_fixture_1"),
        "artifact lane did not promote its own fixture:\n{calls}"
    );
    assert!(
        calls.lines().all(|line| !line.contains("screenshot")),
        "artifact coverage must not invoke the unavailable screenshot producer:\n{calls}"
    );
    assert!(
        calls
            .lines()
            .any(|line| { line.contains("artifacts get art_raw_fixture_1 --include-content-url") }),
        "artifact detail received an empty or screenshot-owned id:\n{calls}"
    );
    assert!(results.join("art_raw_fixture_1.bin").is_file());
    assert_eq!(
        fs::metadata(results.join("artifact-range.bin"))
            .unwrap()
            .len(),
        16
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
fn cleanup_reports_external_resource_failures_without_changing_the_exit_status() {
    let temp = tempfile::tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    for command in ["curl", "docker"] {
        let path = bin_dir.join(command);
        fs::write(&path, "#!/usr/bin/env bash\nexit 7\n").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
    let outdir = temp.path().join("results");
    fs::create_dir_all(outdir.join("logs")).unwrap();
    let setup = temp.path().join("setup/.axon/compose");
    fs::create_dir_all(&setup).unwrap();
    fs::write(temp.path().join("setup/.axon/.env"), "").unwrap();
    fs::write(setup.join("docker-compose.yaml"), "").unwrap();
    fs::write(setup.join("docker-compose.external-qdrant.yaml"), "").unwrap();
    fs::write(setup.join("docker-compose.external-providers.yaml"), "").unwrap();

    let shell = r#"
set -uo pipefail
failures=0
live_chrome_pid="99999999"
live_chrome_pgid="99999999"
live_chrome_start_time="1"
live_chrome_session_token="owned-test-token"
isolated_compose_project=axon-live-test
isolated_compose_network=axon-live-test
isolated_collections=(axon_live_test)
source scripts/lib/live-cli-reporting.sh
cleanup_live_fixtures
printf 'failures=%s\n' "$failures"
"#;
    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap());
    let output = Command::new("bash")
        .args(["-c", shell])
        .env("PATH", path)
        .env("OUTDIR", &outdir)
        .env("SETUP_HOME", temp.path().join("setup"))
        .env("QDRANT_URL", "http://qdrant.invalid")
        .output()
        .expect("run cleanup failure probe");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "failures=0\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    for resource in [
        "Chrome process group (ownership identity unavailable)",
        "compose stack",
        "Docker network",
        "Qdrant collection",
    ] {
        assert!(
            stderr.contains(resource),
            "missing {resource} cleanup warning: {stderr}"
        );
    }
}

#[test]
fn scenario_mode_rejects_collection_names_that_can_escape_the_qdrant_path_segment() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("commands.json");
    fs::write(&registry, r#"{"commands":[]}"#).unwrap();
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let fake_axon = bin_dir.join("axon");
    fs::write(&fake_axon, "#!/usr/bin/env bash\nprintf '{}\\n'\n").unwrap();
    let fake_curl = bin_dir.join("curl");
    fs::write(
        &fake_curl,
        "#!/usr/bin/env bash\nif [[ \"$*\" == *':9222/json/version'* ]]; then printf '{\"Browser\":\"HeadlessChrome/1\",\"webSocketDebuggerUrl\":\"ws://127.0.0.1:9222/devtools/browser/test\"}\\n'; else printf '{}\\n'; fi\n",
    )
    .unwrap();
    for path in [&fake_axon, &fake_curl] {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap());

    for collection in [
        "axon_live_test/../../collections/production",
        "axon_live_test?timeout=0",
        "axon_live_test#fragment",
        "axon_live_test%2fproduction",
    ] {
        let output = Command::new("bash")
            .arg("scripts/live-test-all-commands.sh")
            .args(["--mode", "scenarios"])
            .env("PATH", &path)
            .env("AXON_BIN", &fake_axon)
            .env("AXON_COMMAND_REGISTRY", &registry)
            .env(
                "AXON_LIVE_TEST_OUTDIR",
                temp.path().join(format!("results-{}", collection.len())),
            )
            .env("AXON_LIVE_COLLECTION", collection)
            .output()
            .expect("run scenario harness with unsafe collection name");
        assert_eq!(
            output.status.code(),
            Some(2),
            "unsafe collection accepted: {collection}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("isolated live collection must match"),
            "missing strict collection diagnostic for {collection}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
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

#[test]
fn screenshot_scenario_proves_every_global_capture_option() {
    let scenarios = fs::read_to_string("scripts/lib/live-cli-scenarios-jobs-source.sh")
        .expect("read screenshot scenarios");

    for option in ["--output", "--viewport", "--screenshot-full-page"] {
        assert!(
            scenarios.contains(&format!("prove_option_behavior \"@global\" \"{option}\"")),
            "screenshot scenario must record behavioral evidence for {option}"
        );
    }
}
