use super::*;
use serial_test::serial;

/// Verify that setting AXON_IN_CONTAINER=1 causes running_in_container() to
/// return true without touching the filesystem.
#[allow(unsafe_code)]
#[test]
#[serial]
fn running_in_container_respects_env_var() {
    // SAFETY: test-only mutation; #[serial] ensures no concurrent test reads this var.
    unsafe {
        std::env::set_var("AXON_IN_CONTAINER", "1");
    }
    let result = running_in_container();
    unsafe {
        std::env::remove_var("AXON_IN_CONTAINER");
    }
    assert!(result, "should return true when AXON_IN_CONTAINER=1");
}

/// Verify truthy aliases are also accepted.
#[allow(unsafe_code)]
#[test]
#[serial]
fn running_in_container_accepts_truthy_aliases() {
    for value in ["true", "TRUE", "yes", "YES"] {
        unsafe {
            std::env::set_var("AXON_IN_CONTAINER", value);
        }
        let result = running_in_container();
        unsafe {
            std::env::remove_var("AXON_IN_CONTAINER");
        }
        assert!(result, "should return true when AXON_IN_CONTAINER={value}");
    }
}

/// Verify that non-truthy values are rejected.
#[allow(unsafe_code)]
#[test]
#[serial]
fn running_in_container_rejects_nontruthy_values() {
    for value in ["0", "false", "no", "off", "2"] {
        unsafe {
            std::env::set_var("AXON_IN_CONTAINER", value);
        }
        let result = running_in_container();
        unsafe {
            std::env::remove_var("AXON_IN_CONTAINER");
        }
        assert!(
            !result,
            "should return false when AXON_IN_CONTAINER={value}"
        );
    }
}

/// Verify that without any container signals the function returns false.
///
/// Marked #[ignore] because this test may legitimately fail when the test
/// runner itself executes inside a Docker container (e.g. CI), where
/// `/.dockerenv` or `/run/.containerenv` are present on the filesystem.
/// Run with `cargo test -- --ignored` only on a bare-metal host.
#[allow(unsafe_code)]
#[test]
#[serial]
#[ignore = "/.dockerenv or /run/.containerenv may be present in CI containers"]
fn running_in_container_false_when_no_signals() {
    // Ensure the env var is unset so we fall through to filesystem checks.
    unsafe {
        std::env::remove_var("AXON_IN_CONTAINER");
    }
    assert!(
        !running_in_container(),
        "should return false when no container signals are present"
    );
}
