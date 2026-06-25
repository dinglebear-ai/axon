use super::*;
use std::sync::{LazyLock, Mutex};

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// RAII guard that clears the Chrome diagnostics env vars on drop, ensuring
/// cleanup runs even if the test panics mid-assertion.
struct EnvCleanupGuard;

impl Drop for EnvCleanupGuard {
    fn drop(&mut self) {
        // SAFETY: The caller holds ENV_LOCK for the duration of this guard's
        // lifetime, ensuring single-threaded access to these env vars.
        #[allow(unsafe_code)]
        unsafe {
            env::remove_var("AXON_CHROME_DIAGNOSTICS");
            env::remove_var("AXON_CHROME_DIAGNOSTICS_SCREENSHOT");
            env::remove_var("AXON_CHROME_DIAGNOSTICS_EVENTS");
            env::remove_var("AXON_CHROME_DIAGNOSTICS_DIR");
        }
    }
}

fn with_env_lock<T>(f: impl FnOnce() -> T) -> T {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Drop cleanup guard after f() returns (or panics), before releasing the lock.
    let _cleanup = EnvCleanupGuard;
    f()
}

#[test]
fn diagnostics_defaults_to_disabled_with_cache_dir() {
    with_env_lock(|| {
        let pattern = browser_diagnostics_pattern();
        let expected_output_dir = axon_data_dir()
            .map(|d| format!("{}/chrome-diagnostics", d.display()))
            .unwrap_or_else(|| ".cache/chrome-diagnostics".to_string());
        assert!(!pattern.enabled);
        assert!(!pattern.screenshot);
        assert!(!pattern.events);
        assert_eq!(pattern.output_dir, expected_output_dir);
    });
}

#[test]
fn diagnostics_enables_screenshot_events_when_global_flag_set() {
    with_env_lock(|| {
        // SAFETY: Tests use ENV_LOCK to ensure single-threaded access to env vars.
        #[allow(unsafe_code)]
        unsafe {
            env::set_var("AXON_CHROME_DIAGNOSTICS", "true");
        }
        let pattern = browser_diagnostics_pattern();
        assert!(pattern.enabled);
        assert!(pattern.screenshot);
        assert!(pattern.events);
    });
}

#[test]
fn diagnostics_allows_per_signal_override() {
    with_env_lock(|| {
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
    });
}
