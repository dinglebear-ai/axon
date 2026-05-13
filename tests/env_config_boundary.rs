use std::process::Command;

#[test]
fn env_config_boundary_matrix_is_current() {
    let output = Command::new("python3")
        .arg("scripts/check-env-config-boundary.py")
        .output()
        .expect("run env boundary checker");

    assert!(
        output.status.success(),
        "checker failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
