use super::*;

#[tokio::test]
async fn unused_empty_pool_is_evicted_after_idle_interval() {
    let directory = tempfile::tempdir().unwrap();
    let backend = LlmBackendConfig {
        codex_cmd: directory.path().join("unused-model").display().to_string(),
        ..LlmBackendConfig::default()
    };
    let key = format!(
        "{}\0{}",
        backend.codex_cmd,
        backend.codex_model.as_deref().unwrap_or("")
    );
    let pool = CodexPool::new(1, Duration::from_millis(20), backend);
    POOL_MAP.insert(key.clone(), pool.clone());
    drop(pool);
    tokio::time::sleep(Duration::from_millis(150)).await;
    let retained = POOL_MAP.contains_key(&key);
    POOL_MAP.remove(&key);
    assert!(
        !retained,
        "unused pool metadata must not live for the entire daemon lifetime"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn idle_expiry_reclaims_child_without_another_checkout() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("idle-codex");
    std::fs::write(&script, "#!/bin/sh\necho '{\"id\":0,\"result\":{\"userAgent\":\"fake\"}}'\necho '{\"id\":1,\"result\":{\"thread\":{\"id\":\"idle\"},\"model\":\"fake\"}}'\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let backend = LlmBackendConfig {
        codex_cmd: script.display().to_string(),
        ..LlmBackendConfig::default()
    };
    let pool = CodexPool::new(1, Duration::from_millis(50), backend);
    let slot = pool.checkout(Duration::from_secs(5)).await.unwrap();
    let home = slot._home_guard.as_ref().unwrap().path().to_path_buf();
    pool.checkin(slot).await;
    // Reaping runs in a separate task and includes asynchronous process cleanup.
    // Observe both effects rather than assuming the task ran within 250ms on CI.
    tokio::time::timeout(Duration::from_secs(5), async {
        while pool.metrics().await.idle != 0 || home.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("idle reaper must reclaim the child and its home without another checkout");
    assert_eq!(
        pool.metrics().await.idle,
        0,
        "idle TTL must not require another request"
    );
    assert!(!home.exists(), "expired child home must be reclaimed");
}

#[cfg(unix)]
#[tokio::test]
async fn canceled_initialization_releases_spawning_accounting() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("pending-codex");
    std::fs::write(&script, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let backend = LlmBackendConfig {
        codex_cmd: script.display().to_string(),
        ..LlmBackendConfig::default()
    };
    let pool = CodexPool::new(1, Duration::from_secs(30), backend);
    let timed_out = tokio::time::timeout(
        Duration::from_millis(100),
        pool.checkout(Duration::from_secs(30)),
    )
    .await
    .is_err();
    assert!(timed_out, "fake child must hold initialization open");
    assert_eq!(pool.metrics().await.spawning, 0);
    assert_eq!(pool.metrics().await.active_or_spawning, 0);
}

#[tokio::test]
async fn canceled_checkout_releases_waiter_admission() {
    let pool = CodexPool::new(1, Duration::from_secs(30), LlmBackendConfig::default());
    let held = pool.permits.clone().acquire_owned().await.unwrap();
    for _ in 0..12 {
        let checkout = pool.checkout(Duration::from_secs(30));
        tokio::pin!(checkout);
        assert!(futures_util::poll!(&mut checkout).is_pending());
        assert_eq!(pool.metrics().await.waiting, 1);
        // The actual future is dropped at the end of this iteration.
    }
    drop(held);
    assert_eq!(pool.metrics().await.waiting, 0);
    assert_eq!(pool.metrics().await.rejected, 0);
}

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
        kind: crate::runtime::LlmBackendKind::CodexAppServer,
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
    assert_eq!(
        pool.metrics().await,
        PoolMetrics {
            active_or_spawning: 1,
            spawning: 0,
            idle: 0,
            waiting: 0,
            rejected: 0,
        }
    );
    let mut collected = String::new();
    let r1 = run_turn(&mut slot, "prompt1", None, None, &backend, &mut |d| {
        collected.push_str(d);
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(r1.text, "turn1");
    pool.checkin(slot).await;
    assert_eq!(pool.metrics().await.idle, 1);
    assert_eq!(pool.metrics().await.active_or_spawning, 0);

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
