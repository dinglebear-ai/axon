use super::*;
use std::sync::{LazyLock, Mutex};

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn with_env_lock<T>(f: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    f()
}

#[allow(unsafe_code)]
fn reset_env() {
    // SAFETY: Tests use ENV_LOCK to ensure single-threaded access to env vars.
    unsafe {
        env::remove_var("AXON_CHROME_DIAGNOSTICS");
        env::remove_var("AXON_CHROME_DIAGNOSTICS_SCREENSHOT");
        env::remove_var("AXON_CHROME_DIAGNOSTICS_EVENTS");
        env::remove_var("AXON_CHROME_DIAGNOSTICS_DIR");
    }
}

#[test]
fn diagnostics_defaults_to_disabled_with_cache_dir() {
    with_env_lock(|| {
        reset_env();
        let pattern = browser_diagnostics_pattern();
        let expected_output_dir = axon_data_dir()
            .map(|d| format!("{}/chrome-diagnostics", d.display()))
            .unwrap_or_else(|| ".cache/chrome-diagnostics".to_string());
        assert!(!pattern.enabled);
        assert!(!pattern.screenshot);
        assert!(!pattern.events);
        assert_eq!(pattern.output_dir, expected_output_dir);
        reset_env();
    });
}

#[test]
fn diagnostics_enables_screenshot_events_when_global_flag_set() {
    with_env_lock(|| {
        reset_env();
        // SAFETY: Tests use ENV_LOCK to ensure single-threaded access to env vars.
        #[allow(unsafe_code)]
        unsafe {
            env::set_var("AXON_CHROME_DIAGNOSTICS", "true")
        };
        let pattern = browser_diagnostics_pattern();
        assert!(pattern.enabled);
        assert!(pattern.screenshot);
        assert!(pattern.events);
        reset_env();
    });
}

#[test]
fn diagnostics_allows_per_signal_override() {
    with_env_lock(|| {
        reset_env();
        // SAFETY: Tests use ENV_LOCK to ensure single-threaded access to env vars.
        #[allow(unsafe_code)]
        unsafe {
            env::set_var("AXON_CHROME_DIAGNOSTICS", "true");
            env::set_var("AXON_CHROME_DIAGNOSTICS_EVENTS", "false");
            env::set_var("AXON_CHROME_DIAGNOSTICS_DIR", "/tmp/diag");
        }
        let pattern = browser_diagnostics_pattern();
        assert!(pattern.enabled);
        assert!(pattern.screenshot);
        assert!(!pattern.events);
        assert_eq!(pattern.output_dir, "/tmp/diag");
        reset_env();
    });
}
