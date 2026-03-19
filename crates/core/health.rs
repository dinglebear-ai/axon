pub mod doctor;

pub use doctor::build_doctor_report;

use crate::crates::core::config::parse::helpers::env_bool;
use crate::crates::core::logging::log_info;
use crate::crates::core::paths::axon_data_dir;
use std::env;
use std::time::Duration;
use std::time::Instant;

const DIAGNOSTICS_DIR_DEFAULT: &str = ".cache/chrome-diagnostics";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserDiagnosticsPattern {
    pub enabled: bool,
    pub screenshot: bool,
    pub events: bool,
    pub output_dir: String,
}

pub async fn redis_healthy(redis_url: &str) -> bool {
    let probe_start = Instant::now();
    let url =
        crate::crates::core::config::parse::normalize_local_service_url(redis_url.to_string());
    // Redact credentials — the raw URL may contain a password; log only host:port.
    let host_label = reqwest::Url::parse(&url)
        .ok()
        .and_then(|u: reqwest::Url| {
            u.host_str()
                .map(|h| format!("{}:{}", h, u.port().unwrap_or(6379)))
        })
        .unwrap_or_else(|| "unknown".to_string());
    let client = match redis::Client::open(url.as_str()) {
        Ok(client) => client,
        Err(_) => {
            log_info(&format!(
                "health_probe service=redis host={host_label} result=false duration_ms={}",
                probe_start.elapsed().as_millis()
            ));
            return false;
        }
    };

    let ping = async {
        let mut conn = client.get_multiplexed_async_connection().await?;
        redis::cmd("PING")
            .query_async::<String>(&mut conn)
            .await
            .map(|_| ())
    };

    let result = matches!(
        tokio::time::timeout(Duration::from_secs(5), ping).await,
        Ok(Ok(()))
    );
    log_info(&format!(
        "health_probe service=redis host={host_label} result={result} duration_ms={}",
        probe_start.elapsed().as_millis()
    ));
    result
}

pub fn browser_diagnostics_pattern() -> BrowserDiagnosticsPattern {
    let enabled = env_bool("AXON_CHROME_DIAGNOSTICS", false);
    let screenshot = env_bool("AXON_CHROME_DIAGNOSTICS_SCREENSHOT", enabled);
    let events = env_bool("AXON_CHROME_DIAGNOSTICS_EVENTS", enabled);

    let output_dir = env::var("AXON_CHROME_DIAGNOSTICS_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| axon_data_dir().map(|d| format!("{}/axon/chrome-diagnostics", d.display())))
        .unwrap_or_else(|| DIAGNOSTICS_DIR_DEFAULT.to_string());

    BrowserDiagnosticsPattern {
        enabled,
        screenshot,
        events,
        output_dir,
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn with_env_lock<T>(f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        f()
    }

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
                .map(|d| format!("{}/axon/chrome-diagnostics", d.display()))
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
            unsafe { env::set_var("AXON_CHROME_DIAGNOSTICS", "true") };
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
}
