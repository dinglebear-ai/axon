use std::process::{Command, Output};
use std::time::{Duration, Instant};

fn temp_home() -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("temp home");
    let axon_home = home.path().join(".axon");
    std::fs::create_dir_all(&axon_home).expect("mkdir .axon");
    std::fs::write(
        axon_home.join(".env"),
        "QDRANT_URL=http://127.0.0.1:53333\nTEI_URL=http://127.0.0.1:52000\n",
    )
    .expect("write .env");
    home
}

fn axon(home: &tempfile::TempDir) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_axon"));
    cmd.env("HOME", home.path())
        .env_remove("AXON_ENV_FILE")
        .env_remove("AXON_CONFIG_PATH")
        .env_remove("QDRANT_URL")
        .env_remove("TEI_URL")
        .env_remove("NO_COLOR")
        .env_remove("FORCE_COLOR")
        .env_remove("CLICOLOR_FORCE");
    cmd
}

fn output_with_timeout(mut cmd: Command, timeout: Duration) -> Output {
    let mut child = cmd.spawn().expect("spawn axon");
    let started = Instant::now();

    loop {
        if child.try_wait().expect("poll axon").is_some() {
            return child.wait_with_output().expect("collect axon output");
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let output = child.wait_with_output().expect("collect timed-out output");
            panic!(
                "axon command timed out after {timeout:?}\nstdout={}\nstderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn status_watch_with_server_url_stays_on_local_path() {
    let home = temp_home();
    let mut cmd = axon(&home);
    cmd.env("AXON_SERVER_URL", "http://127.0.0.1:9")
        .arg("--color=never")
        .arg("status")
        .arg("--watch");
    let output = output_with_timeout(cmd, Duration::from_secs(8));

    assert!(
        output.status.success(),
        "status --watch should not attempt the dead server, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("server mode status failed"),
        "watch mode should bypass server dispatch, stderr={stderr}"
    );
}

#[test]
fn status_watch_json_and_quiet_use_one_shot_output() {
    let home = temp_home();
    let json_output = axon(&home)
        .arg("--local")
        .arg("--color=never")
        .arg("status")
        .arg("--watch")
        .arg("--json")
        .output()
        .expect("run axon status --watch --json");
    assert!(
        json_output.status.success(),
        "json status failed: {}",
        String::from_utf8_lossy(&json_output.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&json_output.stdout)
        .expect("watch+json must remain parseable JSON");

    let quiet_output = axon(&home)
        .arg("--local")
        .arg("--color=never")
        .arg("--quiet")
        .arg("status")
        .arg("--watch")
        .output()
        .expect("run axon status --watch --quiet");
    assert!(
        quiet_output.status.success(),
        "quiet status failed: {}",
        String::from_utf8_lossy(&quiet_output.stderr)
    );
    let stdout = String::from_utf8_lossy(&quiet_output.stdout);
    assert!(stdout.contains("Crawl") || stdout.contains("None."));
    assert!(!stdout.contains("live view"));
}

#[test]
fn color_auto_does_not_emit_ansi_to_piped_stdout_but_always_does() {
    let home = temp_home();
    let auto_output = axon(&home)
        .arg("--local")
        .arg("--color=auto")
        .arg("status")
        .output()
        .expect("run axon status --color=auto");
    assert!(auto_output.status.success());
    let auto_stdout = String::from_utf8_lossy(&auto_output.stdout);
    assert!(
        !auto_stdout.contains("\x1b["),
        "piped stdout must stay plain in auto mode: {auto_stdout:?}"
    );

    let always_output = axon(&home)
        .arg("--local")
        .arg("--color=always")
        .arg("status")
        .output()
        .expect("run axon status --color=always");
    assert!(always_output.status.success());
    let always_stdout = String::from_utf8_lossy(&always_output.stdout);
    assert!(
        always_stdout.contains("\x1b["),
        "always mode should force ANSI in stdout: {always_stdout:?}"
    );
}

#[test]
fn color_never_strips_stderr_logging() {
    let home = temp_home();
    let never_output = axon(&home)
        .arg("--local")
        .arg("--color=never")
        .arg("status")
        .arg("--json")
        .output()
        .expect("run axon status --color=never --json");
    assert!(never_output.status.success());
    let never_stderr = String::from_utf8_lossy(&never_output.stderr);
    assert!(
        !never_stderr.contains("\x1b["),
        "never mode must strip stderr ANSI: {never_stderr:?}"
    );
}

#[test]
fn watch_is_rejected_for_non_status_commands() {
    let home = temp_home();
    let output = axon(&home)
        .arg("--local")
        .arg("sources")
        .arg("--watch")
        .output()
        .expect("run axon sources --watch");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--watch is only supported with `axon status`"));
}
