use super::*;
use serial_test::serial;

/// Save `AXON_IN_CONTAINER`, run `f`, then restore the original value so
/// later tests (and the process environment) are not polluted.
#[allow(unsafe_code)]
fn with_container_env<F: FnOnce()>(value: Option<&str>, f: F) {
    let orig = std::env::var("AXON_IN_CONTAINER").ok();
    // SAFETY: test-only mutation; callers must use #[serial].
    unsafe {
        match value {
            Some(v) => std::env::set_var("AXON_IN_CONTAINER", v),
            None => std::env::remove_var("AXON_IN_CONTAINER"),
        }
    }
    f();
    unsafe {
        match orig.as_deref() {
            Some(v) => std::env::set_var("AXON_IN_CONTAINER", v),
            None => std::env::remove_var("AXON_IN_CONTAINER"),
        }
    }
}

/// Verify that setting AXON_IN_CONTAINER=1 causes running_in_container() to
/// return true without touching the filesystem.
#[test]
#[serial]
fn running_in_container_respects_env_var() {
    with_container_env(Some("1"), || {
        assert!(
            running_in_container(),
            "should return true when AXON_IN_CONTAINER=1"
        );
    });
}

/// Verify truthy aliases are also accepted.
#[test]
#[serial]
fn running_in_container_accepts_truthy_aliases() {
    for value in ["true", "TRUE", "yes", "YES"] {
        with_container_env(Some(value), || {
            assert!(
                running_in_container(),
                "should return true when AXON_IN_CONTAINER={value}"
            );
        });
    }
}

/// Verify that non-truthy values are rejected.
#[test]
#[serial]
fn running_in_container_rejects_nontruthy_values() {
    for value in ["0", "false", "no", "off", "2"] {
        with_container_env(Some(value), || {
            assert!(
                !running_in_container(),
                "should return false when AXON_IN_CONTAINER={value}"
            );
        });
    }
}

/// Verify that without any container signals the function returns false.
///
/// Marked #[ignore] because this test may legitimately fail when the test
/// runner itself executes inside a Docker container (e.g. CI), where
/// `/.dockerenv` or `/run/.containerenv` are present on the filesystem.
/// Run with `cargo test -- --ignored` only on a bare-metal host.
#[test]
#[serial]
#[ignore = "/.dockerenv or /run/.containerenv may be present in CI containers"]
fn running_in_container_false_when_no_signals() {
    with_container_env(None, || {
        assert!(
            !running_in_container(),
            "should return false when no container signals are present"
        );
    });
}
