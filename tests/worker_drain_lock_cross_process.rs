//! Cross-process regression for the worker drain lock (`axon_rust-x4gxr.2/.3`).
//!
//! The in-process unit tests in `axon-services` verify SQLite's same-process
//! lock bookkeeping, but the feature's real requirement is *cross-process*
//! mutual exclusion: exactly one `axon jobs worker` may hold the lock at a time,
//! so a second worker (auto-spawned or manual) exits immediately while a server
//! or another worker is alive. sqlx defaults SQLite to WAL, where a read-only
//! `BEGIN EXCLUSIVE` does not take a cross-process lock — this test would fail
//! under that default and passes with the rollback-journal fix.
//!
//! It spawns a real `axon jobs worker` as a managed child (kept alive for the
//! test's duration), waits for it to acquire the lock, then runs a second
//! worker and asserts it refuses.

use std::process::Command;
use std::time::Duration;

fn axon_bin() -> &'static str {
    env!("CARGO_BIN_EXE_axon")
}

#[test]
fn second_worker_refuses_while_first_holds_the_lock() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path();

    // Holder: runs forever (idle-exit 0). The drain lock is acquired as the
    // very first thing in `run_worker_process`, before any runtime is built, so
    // a short fixed head-start guarantees it is held. (Its stdout is
    // block-buffered to a pipe when not a tty, so we can't read the banner
    // live — the second worker's refusal is the actual assertion.)
    let mut holder = worker_command()
        .args(["jobs", "worker", "--idle-exit-secs", "0"])
        .env("AXON_DATA_DIR", data_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn holder worker");

    // The lock is acquired first thing in `run_worker_process`, before any
    // runtime is built, so a short fixed head-start guarantees it is held.
    std::thread::sleep(Duration::from_secs(3));
    if let Ok(Some(status)) = holder.try_wait() {
        panic!("holder worker exited early before it could hold the lock: {status:?}");
    }

    // Second worker: must detect the held lock and exit immediately. It refuses
    // at `try_hold`, before building any runtime, so `.output()` returns fast.
    let second = worker_command()
        .args(["jobs", "worker", "--idle-exit-secs", "1", "--json"])
        .env("AXON_DATA_DIR", data_dir)
        .output()
        .expect("run second worker");

    let stdout = String::from_utf8_lossy(&second.stdout);
    let _ = holder.kill();
    let _ = holder.wait();

    assert!(
        stdout.contains("\"acquired_lock\": false") || stdout.contains("\"acquired_lock\":false"),
        "second worker should refuse while the first holds the lock; stdout: {stdout}"
    );
    assert!(
        second.status.success(),
        "second worker should exit 0 (clean refusal), got {:?}",
        second.status
    );
    // Lock release on holder death is a kernel guarantee for the fcntl lock and
    // is covered in-process by `drain_lock_tests::probe_does_not_evict_a_live_holder`
    // / `holder_excludes_second_holder_until_dropped`; not re-tested here because
    // a reacquiring worker would not cleanly idle-exit against dummy endpoints.
}

/// A `Command` for the axon binary with dummy (present-but-unreachable) service
/// endpoints. The worker requires `QDRANT_URL`/`TEI_URL` to be *present* at
/// config-parse time; they are never contacted on the lock/refusal paths under
/// test. `AXON_ALLOW_INCOMPATIBLE_STORE_STARTUP` bypasses the reachability gate
/// so the holder can start and hold the lock.
fn worker_command() -> Command {
    let mut cmd = Command::new(axon_bin());
    cmd.env("QDRANT_URL", "http://127.0.0.1:1")
        .env("TEI_URL", "http://127.0.0.1:1")
        .env("AXON_ALLOW_INCOMPATIBLE_STORE_STARTUP", "1");
    cmd
}
