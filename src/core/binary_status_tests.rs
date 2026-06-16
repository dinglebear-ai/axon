use super::{stale_binary_warning_for, warning_message};
use std::time::{Duration, SystemTime};

#[test]
fn stale_binary_warning_reports_newer_input() {
    let binary_mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
    let input_mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(20);

    let warning = stale_binary_warning_for(binary_mtime, [("src/lib.rs", input_mtime)])
        .expect("newer input should warn");

    assert!(warning.contains("outdated axon binary"));
    assert!(warning.contains("src/lib.rs"));
    assert!(warning.contains("cargo build"));
}

#[test]
fn stale_binary_warning_is_absent_when_binary_is_current() {
    let binary_mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(20);
    let input_mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(10);

    assert!(stale_binary_warning_for(binary_mtime, [("src/lib.rs", input_mtime)]).is_none());
}

#[test]
fn warning_message_mentions_requested_destinations() {
    let warning = warning_message("src/main.rs");

    assert!(warning.contains("~/.local/bin/axon"));
}
