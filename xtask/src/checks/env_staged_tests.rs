use super::is_violation;

#[test]
fn is_violation_blocks_dot_env() {
    assert!(is_violation(".env"));
}

#[test]
fn is_violation_allows_dot_env_example() {
    assert!(!is_violation(".env.example"));
}

#[test]
fn is_violation_blocks_dot_env_local() {
    assert!(is_violation(".env.local"));
}

#[test]
fn is_violation_blocks_dot_env_production() {
    assert!(is_violation(".env.production"));
}

#[test]
fn is_violation_blocks_services_env() {
    assert!(is_violation("services.env"));
}

#[test]
fn is_violation_blocks_arbitrary_dot_env_suffix() {
    assert!(is_violation("prod.env"));
    assert!(is_violation("staging.env"));
}

#[test]
fn is_violation_allows_unrelated_files() {
    assert!(!is_violation("Cargo.toml"));
    assert!(!is_violation("src/main.rs"));
    assert!(!is_violation("README.md"));
    assert!(!is_violation("envoy.yaml"));
}
