use super::*;

// Non-`AXON_`-prefixed test keys: the env-config-boundary checker greps source
// for `AXON_*` literals and requires each in the migration matrix, so test-only
// fake var names must not use that prefix. `overlay_or_env` is name-agnostic, so
// these arbitrary keys exercise the same overlay-vs-env precedence logic.
const UNSET_TEST_KEY: &str = "OVERLAY_OR_ENV_TEST_DEFINITELY_UNSET";
const FALLBACK_TEST_KEY: &str = "OVERLAY_OR_ENV_TEST_FALLBACK";

#[test]
fn overlay_or_env_prefers_overlay_over_env() {
    // When the active profile supplies a value, the env var is never consulted —
    // this is the "active profile overrides env" guarantee at the field level.
    assert_eq!(
        overlay_or_env(&Some("from-profile".to_string()), UNSET_TEST_KEY),
        Some("from-profile".to_string())
    );
}

#[test]
fn overlay_or_env_none_when_both_absent() {
    assert_eq!(overlay_or_env(&None, UNSET_TEST_KEY), None);
}

#[test]
#[serial_test::serial]
fn overlay_or_env_falls_back_to_env_when_overlay_none() {
    let key = FALLBACK_TEST_KEY;
    let prev = env::var(key).ok();
    #[allow(unsafe_code)]
    unsafe {
        env::set_var(key, "from-env");
    }
    assert_eq!(overlay_or_env(&None, key), Some("from-env".to_string()));
    #[allow(unsafe_code)]
    match prev {
        Some(v) => unsafe { env::set_var(key, v) },
        None => unsafe { env::remove_var(key) },
    }
}
