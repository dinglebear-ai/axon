use super::*;
use crate::core::llm::LlmBackendKind;
use std::io;
use std::time::Duration;

fn backend_with_cmd(cmd: &str) -> LlmBackendConfig {
    LlmBackendConfig {
        codex_cmd: cmd.to_string(),
        ..LlmBackendConfig::default()
    }
}

#[test]
fn completion_timeout_is_at_least_one_second() {
    let zero = LlmBackendConfig {
        completion_timeout_secs: 0,
        ..LlmBackendConfig::default()
    };
    assert_eq!(zero.completion_timeout(), Duration::from_secs(1));
    let forty_five = LlmBackendConfig {
        completion_timeout_secs: 45,
        ..LlmBackendConfig::default()
    };
    assert_eq!(forty_five.completion_timeout(), Duration::from_secs(45));
}

#[test]
fn validate_codex_cmd_allows_bare_name() {
    assert!(validate_codex_cmd(&backend_with_cmd("codex")).is_ok());
}

#[test]
fn validate_codex_cmd_rejects_bare_name_in_container_for_codex_backend() {
    if crate::core::config::parse::docker::running_in_container() {
        let backend = LlmBackendConfig {
            kind: LlmBackendKind::CodexAppServer,
            codex_cmd: "codex".to_string(),
            ..LlmBackendConfig::default()
        };

        let err = validate_codex_cmd(&backend).unwrap_err();

        assert!(err.to_string().contains("host-only"), "got: {err}");
    }
}

#[test]
fn validate_codex_cmd_rejects_empty() {
    assert!(validate_codex_cmd(&backend_with_cmd("   ")).is_err());
}

#[test]
fn validate_codex_cmd_rejects_missing_path() {
    let err = validate_codex_cmd(&backend_with_cmd("/nonexistent/path/to/codex")).unwrap_err();
    assert!(err.to_string().contains("AXON_CODEX_CMD"));
}

#[test]
fn stderr_suffix_empty_for_blank() {
    assert_eq!(stderr_suffix(b""), "");
    assert_eq!(stderr_suffix(b"   \n  "), "");
}

#[test]
fn stderr_suffix_includes_nonblank_tail() {
    let suffix = stderr_suffix(b"something broke");
    assert!(suffix.contains("stderr:"));
    assert!(suffix.contains("something broke"));
}

#[tokio::test]
async fn collect_stderr_reports_join_failure() {
    let task = tokio::spawn(async { std::future::pending::<Result<Vec<u8>, io::Error>>().await });
    task.abort();

    let err = collect_stderr(task).await.unwrap_err();

    assert!(
        err.contains("failed to join codex stderr reader"),
        "got: {err}"
    );
}

#[tokio::test]
async fn collect_stderr_reports_timeout() {
    let task = tokio::spawn(async { std::future::pending::<Result<Vec<u8>, io::Error>>().await });

    let err = collect_stderr(task).await.unwrap_err();

    assert!(
        err.contains("timed out collecting codex stderr"),
        "got: {err}"
    );
}

#[cfg(unix)]
#[test]
fn validate_codex_cmd_rejects_symlink() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("real-codex");
    std::fs::write(&target, "#!/bin/sh\n").unwrap();
    let link = dir.path().join("codex-link");
    symlink(&target, &link).unwrap();
    let err = validate_codex_cmd(&backend_with_cmd(link.to_str().unwrap())).unwrap_err();
    assert!(err.to_string().contains("symlink"), "got: {err}");
}

#[cfg(unix)]
#[test]
fn validate_codex_cmd_rejects_non_executable() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-exec");
    std::fs::write(&file, "x").unwrap(); // default perms have no exec bit
    let err = validate_codex_cmd(&backend_with_cmd(file.to_str().unwrap())).unwrap_err();
    assert!(err.to_string().contains("not executable"), "got: {err}");
}

#[cfg(unix)]
#[tokio::test]
async fn codex_completion_success_drives_fake_app_server_process() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("fake-codex");
    let pid_file = dir.path().join("success.pid");
    std::fs::write(
        &script,
        format!(
            r#"#!/usr/bin/env python3
import json
import os
import sys
import time

with open("{pid_file}", "w", encoding="utf-8") as pid:
    pid.write(str(os.getpid()))

home = os.environ.get("HOME")
codex_home = os.environ.get("CODEX_HOME")
assert home == codex_home, dict(HOME=home, CODEX_HOME=codex_home)
assert os.environ.get("XDG_CONFIG_HOME", "").startswith(codex_home), os.environ.get("XDG_CONFIG_HOME")
assert os.environ.get("XDG_CACHE_HOME", "").startswith(codex_home), os.environ.get("XDG_CACHE_HOME")
assert os.environ.get("XDG_DATA_HOME", "").startswith(codex_home), os.environ.get("XDG_DATA_HOME")

def read_msg():
    line = sys.stdin.readline()
    if not line:
        raise SystemExit("stdin closed before protocol completed")
    return json.loads(line)

def send(obj):
    print(json.dumps(obj, separators=(",", ":")), flush=True)

msg = read_msg()
assert msg["method"] == "initialize", msg
send({{"id": 0, "result": {{"userAgent": "fake-codex"}}}})

msg = read_msg()
assert msg["method"] == "initialized", msg
msg = read_msg()
assert msg["method"] == "thread/start", msg
assert msg["params"]["approvalPolicy"] == "never", msg
assert msg["params"]["sandbox"] == "read-only", msg
assert msg["params"]["model"] == "gpt-5.5", msg
send({{"id": 1, "result": {{"thread": {{"id": "thr_success"}}, "model": "gpt-5.5"}}}})

msg = read_msg()
assert msg["method"] == "turn/start", msg
assert msg["params"]["threadId"] == "thr_success", msg
assert msg["params"]["input"][0]["text"] == "system prompt\n\nuser prompt", msg

send({{"method": "item/agentMessage/delta", "params": {{"delta": "Hello "}}}})
send({{"method": "item/agentMessage/delta", "params": {{"delta": "world"}}}})
send({{"method": "thread/tokenUsage/updated", "params": {{"tokenUsage": {{"total": {{"inputTokens": 7, "outputTokens": 3, "totalTokens": 10}}}}}}}})
send({{"method": "item/completed", "params": {{"item": {{"type": "agentMessage", "text": "Hello world"}}}}}})
send({{"method": "turn/completed", "params": {{"turn": {{"status": "completed"}}}}}})

while True:
    time.sleep(1)
"#,
            pid_file = pid_file.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut req = CompletionRequest::new("user prompt").system_prompt("system prompt");
    req.backend = LlmBackendConfig {
        kind: LlmBackendKind::CodexAppServer,
        codex_cmd: script.display().to_string(),
        codex_model: Some("gpt-5.5".to_string()),
        completion_timeout_secs: 3,
        configured: true,
        ..LlmBackendConfig::default()
    };
    let mut deltas = Vec::new();

    let response = complete_streaming(req, |delta| {
        deltas.push(delta.to_string());
        Ok(())
    })
    .await
    .unwrap();

    assert_eq!(response.text, "Hello world");
    assert_eq!(deltas, ["Hello ", "world"]);
    let usage = response.usage.expect("usage captured");
    assert_eq!(usage.prompt_tokens, 7);
    assert_eq!(usage.completion_tokens, 3);
    assert_eq!(usage.total_tokens, 10);
    let pid = std::fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    assert_process_exits(pid);
}

#[cfg(unix)]
#[tokio::test]
async fn codex_completion_timeout_covers_slow_child_before_handshake() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("slow-codex");
    let pid_file = dir.path().join("parent.pid");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\n\
             echo $$ > {}\n\
             cat >/dev/null &\n\
             echo '{{\"id\":0,\"result\":{{\"userAgent\":\"fake\"}}}}'\n\
             echo '{{\"id\":1,\"result\":{{\"thread\":{{\"id\":\"thr_timeout\"}},\"model\":\"fake\"}}}}'\n\
             sleep 5\n",
            pid_file.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut req = CompletionRequest::new("hello");
    req.backend = LlmBackendConfig {
        kind: LlmBackendKind::CodexAppServer,
        codex_cmd: script.display().to_string(),
        completion_timeout_secs: 1,
        configured: true,
        ..LlmBackendConfig::default()
    };

    let started = std::time::Instant::now();
    let err = complete_text(req).await.unwrap_err();

    assert!(started.elapsed() < Duration::from_secs(3));
    let text = err.to_string();
    assert!(text.contains("timed out"), "got: {text}");
    assert!(text.contains("cleanup:"), "got: {text}");
    assert!(text.contains("reaped"), "got: {text}");
    let pid = std::fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    assert_process_exits(pid);
}

#[cfg(unix)]
#[tokio::test]
async fn codex_completion_error_suffix_redacts_compact_stderr_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("stderr-codex");
    std::fs::write(
        &script,
        "#!/bin/sh\n\
         echo '{\"api_key\":\"sk-compact-secret\",\"token\":\"atk_compact_secret\"}' >&2\n\
         exit 1\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut req = CompletionRequest::new("hello");
    req.backend = LlmBackendConfig {
        kind: LlmBackendKind::CodexAppServer,
        codex_cmd: script.display().to_string(),
        completion_timeout_secs: 3,
        configured: true,
        ..LlmBackendConfig::default()
    };

    let err = complete_text(req).await.unwrap_err();
    let text = err.to_string();

    assert!(text.contains("stderr:"), "got: {text}");
    assert!(text.contains("[REDACTED]"), "got: {text}");
    assert!(!text.contains("sk-compact-secret"), "got: {text}");
    assert!(!text.contains("atk_compact_secret"), "got: {text}");
}

#[cfg(unix)]
#[tokio::test]
async fn codex_completion_timeout_kills_grandchild_process_group() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("grandchild-codex");
    let parent_pid_file = dir.path().join("parent.pid");
    let grandchild_pid_file = dir.path().join("grandchild.pid");
    std::fs::write(
        &script,
        format!(
            r#"#!/usr/bin/env python3
import json
import os
import subprocess
import sys
import time

with open("{parent_pid_file}", "w", encoding="utf-8") as pid:
    pid.write(str(os.getpid()))
grandchild = subprocess.Popen(["sleep", "30"])
with open("{grandchild_pid_file}", "w", encoding="utf-8") as pid:
    pid.write(str(grandchild.pid))

print(json.dumps({{"id": 0, "result": {{"userAgent": "fake"}}}}), flush=True)
print(json.dumps({{"id": 1, "result": {{"thread": {{"id": "thr_timeout"}}, "model": "fake"}}}}), flush=True)
time.sleep(30)
"#,
            parent_pid_file = parent_pid_file.display(),
            grandchild_pid_file = grandchild_pid_file.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut req = CompletionRequest::new("hello");
    req.backend = LlmBackendConfig {
        kind: LlmBackendKind::CodexAppServer,
        codex_cmd: script.display().to_string(),
        completion_timeout_secs: 1,
        configured: true,
        ..LlmBackendConfig::default()
    };

    let err = complete_text(req).await.unwrap_err();
    let text = err.to_string();
    assert!(text.contains("timed out"), "got: {text}");
    assert!(text.contains("process group"), "got: {text}");

    let parent_pid = std::fs::read_to_string(parent_pid_file)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    let grandchild_pid = std::fs::read_to_string(grandchild_pid_file)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    assert_process_exits(parent_pid);
    assert_process_exits(grandchild_pid);
}

#[cfg(unix)]
fn assert_process_exits(pid: i32) {
    for _ in 0..40 {
        if !process_is_running(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("process {pid} was still visible after codex timeout cleanup");
}

#[cfg(unix)]
fn process_is_running(pid: i32) -> bool {
    let proc_dir = Path::new("/proc").join(pid.to_string());
    if !proc_dir.exists() {
        return false;
    }
    let Ok(stat) = std::fs::read_to_string(proc_dir.join("stat")) else {
        return true;
    };
    stat.split_whitespace()
        .nth(2)
        .is_none_or(|state| state != "Z")
}
