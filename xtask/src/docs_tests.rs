use super::*;
use std::fs;

#[test]
fn check_aggregates_failures_from_all_three_sub_checks() {
    let dir = tempfile::tempdir().unwrap();
    // No documentation-contract.md, no docs/reference — every sub-check
    // should fail and their messages should all surface.
    fs::write(dir.path().join("README.md"), "[missing](./nope.md)").unwrap();
    let err = check(dir.path()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("nope.md"));
    assert!(msg.contains("documentation-contract.md"));
}
