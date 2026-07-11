use super::*;

fn source(command: &str, argv: &[&str]) -> CliToolSource {
    CliToolSource {
        command: command.to_string(),
        argv: argv.iter().map(|arg| arg.to_string()).collect(),
        env_allowlist: Vec::new(),
        side_effect_class: "none".to_string(),
        timeout_ms: 5_000,
        output_cap_bytes: 64 * 1024,
        audit_metadata: Vec::new(),
    }
}

#[test]
fn executes_a_real_process_with_no_shell() {
    // `/bin/echo` echoes its argv verbatim; if this were routed through a
    // shell, `$HOME` would expand instead of being passed literally.
    let outcome = execute_command(&source("/bin/echo", &["$HOME"])).unwrap();
    assert_eq!(outcome.stdout.trim(), "$HOME");
    assert_eq!(outcome.exit_code, Some(0));
}

#[test]
fn caps_output_at_configured_byte_limit() {
    let mut src = source("/bin/echo", &["hello world"]);
    src.output_cap_bytes = 4;
    let outcome = execute_command(&src).unwrap();
    assert!(outcome.stdout.len() <= 4);
}

#[test]
fn times_out_a_long_running_process() {
    let mut src = source("/bin/sleep", &["5"]);
    src.timeout_ms = 100;
    let err = execute_command(&src).unwrap_err();
    assert_eq!(err.code, "tool.timeout");
}

#[test]
fn clears_environment_by_default() {
    // With no `env_allowlist`, the child's environment is fully cleared —
    // `AXON_TEST_SECRET` set in this test process must not reach it.
    // SAFETY: single-threaded test-local env mutation, unset before return.
    unsafe {
        std::env::set_var("AXON_TEST_SECRET", "should-not-leak");
    }
    let outcome = execute_command(&source("/usr/bin/env", &[])).unwrap();
    unsafe {
        std::env::remove_var("AXON_TEST_SECRET");
    }
    assert!(!outcome.stdout.contains("AXON_TEST_SECRET"));
}

#[test]
fn rejects_unknown_command() {
    let err = execute_command(&source("/this/does/not/exist", &[])).unwrap_err();
    assert_eq!(err.code, "tool.spawn_failed");
}
