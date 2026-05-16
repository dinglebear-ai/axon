use super::*;
use std::env;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn validate_collection_name_accepts_normal_names() {
    for ok in ["cortex", "axon", "axon_v2", "axon-test", "Mem0.v1", "a"] {
        assert!(
            validate_collection_name(ok).is_ok(),
            "expected '{ok}' to be accepted"
        );
    }
}

#[test]
fn validate_collection_name_rejects_path_traversal() {
    for bad in ["..", "../foo", "..foo", ""] {
        assert!(
            validate_collection_name(bad).is_err(),
            "expected '{bad}' to be rejected"
        );
    }
}

#[test]
fn validate_collection_name_rejects_url_metacharacters() {
    for bad in [
        "foo/bar", "foo?x=1", "foo#frag", "foo bar", "foo\nbar", "foo%20",
    ] {
        assert!(
            validate_collection_name(bad).is_err(),
            "expected '{bad}' to be rejected"
        );
    }
}

#[test]
fn validate_collection_name_rejects_overlong() {
    let huge = "a".repeat(256);
    assert!(validate_collection_name(&huge).is_err());
}

#[allow(unsafe_code)]
#[test]
fn env_bool_opt_returns_none_when_absent() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe { env::remove_var("AXON_TEST_BOOL_OPT_ABSENT") };
    assert_eq!(env_bool_opt("AXON_TEST_BOOL_OPT_ABSENT"), None);
}

#[allow(unsafe_code)]
#[test]
fn env_bool_opt_returns_some_true_when_set() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe { env::set_var("AXON_TEST_BOOL_OPT_TRUE", "true") };
    assert_eq!(env_bool_opt("AXON_TEST_BOOL_OPT_TRUE"), Some(true));
    unsafe { env::remove_var("AXON_TEST_BOOL_OPT_TRUE") };
}

#[allow(unsafe_code)]
#[test]
fn env_bool_opt_returns_some_false_when_set_to_0() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe { env::set_var("AXON_TEST_BOOL_OPT_FALSE", "0") };
    assert_eq!(env_bool_opt("AXON_TEST_BOOL_OPT_FALSE"), Some(false));
    unsafe { env::remove_var("AXON_TEST_BOOL_OPT_FALSE") };
}
