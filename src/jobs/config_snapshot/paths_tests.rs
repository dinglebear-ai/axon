use super::normalize_container_output_dir;
use crate::core::config::Config;
use std::path::PathBuf;

#[test]
fn maps_default_host_axon_output_dir_for_container_workers() {
    let mut submitted = Config::test_default();
    submitted.output_dir = PathBuf::from("/home/jmagar/.axon/output");

    let mut worker = Config::test_default();
    worker.output_dir = PathBuf::from("/home/axon/.axon/output");

    normalize_container_output_dir(&worker, &mut submitted, true);

    assert_eq!(
        submitted.output_dir,
        PathBuf::from("/home/axon/.axon/output")
    );
}

#[test]
fn keeps_non_default_axon_output_dir_for_container_workers() {
    let mut submitted = Config::test_default();
    submitted.output_dir = PathBuf::from("/mnt/shared/.axon/output");

    let mut worker = Config::test_default();
    worker.output_dir = PathBuf::from("/home/axon/.axon/output");

    normalize_container_output_dir(&worker, &mut submitted, true);

    assert_eq!(
        submitted.output_dir,
        PathBuf::from("/mnt/shared/.axon/output")
    );
}
