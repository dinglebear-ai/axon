use super::*;
use std::fs;

#[test]
fn check_aggregates_failures_from_all_living_doc_checks() {
    let dir = tempfile::tempdir().unwrap();
    // No docs/README.md, action generator, or docs/reference tree: the
    // independent checks should all run and their messages should aggregate.
    fs::write(dir.path().join("README.md"), "[missing](./nope.md)").unwrap();
    let err = check(dir.path()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("nope.md"));
    assert!(msg.contains("docs/README.md"));
    assert!(msg.contains("generate_action_docs.py"));
}
