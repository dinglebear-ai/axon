pub mod doctor;

pub use doctor::{
    LlmDoctorProbe, build_doctor_report, cutover::assert_workers_allowed_by_cutover,
    cutover::build_cutover_block,
};

use crate::config::parse::helpers::env_bool;
use crate::paths::axon_data_dir;
use std::env;

const DIAGNOSTICS_DIR_DEFAULT: &str = ".cache/chrome-diagnostics";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserDiagnosticsPattern {
    pub enabled: bool,
    pub screenshot: bool,
    pub events: bool,
    pub output_dir: String,
}

pub fn browser_diagnostics_pattern() -> BrowserDiagnosticsPattern {
    let enabled = env_bool("AXON_CHROME_DIAGNOSTICS", false);
    let screenshot = env_bool("AXON_CHROME_DIAGNOSTICS_SCREENSHOT", enabled);
    let events = env_bool("AXON_CHROME_DIAGNOSTICS_EVENTS", enabled);

    let output_dir = env::var("AXON_CHROME_DIAGNOSTICS_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| axon_data_dir().map(|d| format!("{}/chrome-diagnostics", d.display())))
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
#[path = "health_tests.rs"]
mod tests;
