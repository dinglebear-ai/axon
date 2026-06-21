use super::*;

#[cfg(unix)]
#[tokio::test]
async fn pool_reuses_child_across_turns() {
    // Fake codex that handles initialize → thread/start once, then serves
    // two turn/start cycles, recording each in output lines.
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("reuse-codex");
    std::fs::write(
        &script,
        r#"#!/usr/bin/env python3
import json, sys

def read():
    line = sys.stdin.readline()
    if not line:
        raise SystemExit("stdin closed early")
    return json.loads(line)

def send(o):
    print(json.dumps(o, separators=(",", ":")), flush=True)

# One-time init
assert read()["method"] == "initialize"
send({"id": 0, "result": {"userAgent": "pool-fake"}})
assert read()["method"] == "initialized"
msg = read()
assert msg["method"] == "thread/start", msg
send({"id": 1, "result": {"thread": {"id": "thr_reuse"}, "model": "fake"}})

# First turn
msg = read()
assert msg["method"] == "turn/start", msg
send({"method": "item/agentMessage/delta", "params": {"delta": "turn1"}})
send({"method": "turn/completed", "params": {"turn": {"status": "completed"}}})

# Second turn (same child — reused by pool)
msg = read()
assert msg["method"] == "turn/start", msg
send({"method": "item/agentMessage/delta", "params": {"delta": "turn2"}})
send({"method": "turn/completed", "params": {"turn": {"status": "completed"}}})

import time; time.sleep(30)
"#,
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    reset_pools_for_tests().await;

    let backend = LlmBackendConfig {
        kind: crate::core::llm::LlmBackendKind::CodexAppServer,
        codex_cmd: script.display().to_string(),
        completion_concurrency: 1,
        completion_timeout_secs: 5,
        configured: true,
        ..LlmBackendConfig::default()
    };

    // First completion — spawns the child.
    let pool = pool_for(&backend);
    let timeout = backend.completion_timeout();
    let mut slot = pool.checkout(timeout).await.unwrap();
    let mut collected = String::new();
    let r1 = run_turn(&mut slot, "prompt1", None, None, &backend, &mut |d| {
        collected.push_str(d);
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(r1.text, "turn1");
    pool.checkin(slot).await;

    // Second completion — must reuse the same child (thread_id stays "thr_reuse").
    let mut slot2 = pool.checkout(timeout).await.unwrap();
    assert_eq!(
        slot2.thread_id, "thr_reuse",
        "pool must reuse the same child"
    );
    let mut collected2 = String::new();
    let r2 = run_turn(&mut slot2, "prompt2", None, None, &backend, &mut |d| {
        collected2.push_str(d);
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(r2.text, "turn2");
    pool.checkin(slot2).await;
}
