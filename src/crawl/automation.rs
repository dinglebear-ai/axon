//! Chrome web-automation scripts for crawls.
//!
//! A user-supplied JSON file maps URL path prefixes to ordered lists of
//! automation steps (click, scroll, wait, fill, evaluate, screenshot) that
//! spider runs against each matching page during a Chrome render — before the
//! page is captured. This unlocks crawling sites that require interaction
//! (cookie/consent banners, "load more" buttons, infinite scroll, simple
//! search-or-filter flows) which a plain fetch can never reach.
//!
//! The on-disk format is a JSON object keyed by path prefix. spider matches a
//! page's URL path against these keys, so `"/"` applies the steps to every
//! page and `"/blog"` only to pages under `/blog`:
//!
//! ```json
//! {
//!   "/": [
//!     { "action": "wait_for", "selector": "main" },
//!     { "action": "click", "selector": "button.accept-cookies" },
//!     { "action": "scroll_y", "pixels": 4000 },
//!     { "action": "wait", "ms": 1500 }
//!   ]
//! }
//! ```
//!
//! Automation only runs on Chrome render paths; it is a no-op for HTTP-only
//! crawls. Wiring lives in `src/crawl/engine/runtime.rs`.

use std::collections::HashMap;
use std::error::Error;
use std::path::Path;

use serde::Deserialize;
use spider::configuration::{AutomationScriptsMap, WebAutomation};

/// One automation action. Internally tagged on `action` with `snake_case`
/// variant names so the JSON reads `{ "action": "scroll_y", "pixels": 2000 }`.
///
/// This is a curated subset of spider's `WebAutomation` — the steps that are
/// useful and safe to drive from a static config file. Coordinate-based clicks,
/// drag gestures, and keyboard-`Type` are intentionally omitted; they depend on
/// rendered layout and are too brittle for declarative crawl config.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AutomationStep {
    /// Execute raw JavaScript in the page context.
    Evaluate { script: String },
    /// Click the first element matching the CSS selector.
    Click { selector: String },
    /// Click every element matching the CSS selector.
    ClickAll { selector: String },
    /// Wait until an element matching the selector appears.
    WaitFor { selector: String },
    /// Wait for an element matching the selector, then click it.
    WaitForAndClick { selector: String },
    /// Sleep for a fixed number of milliseconds.
    Wait { ms: u64 },
    /// Wait for a page navigation to complete.
    WaitForNavigation,
    /// Scroll horizontally by a pixel delta (negative scrolls left).
    ScrollX { pixels: i32 },
    /// Scroll vertically by a pixel delta (negative scrolls up).
    ScrollY { pixels: i32 },
    /// Auto-scroll toward the bottom of the page up to `times` steps.
    InfiniteScroll { times: u32 },
    /// Fill an input element with a value.
    Fill { selector: String, value: String },
    /// Capture a screenshot to `output`.
    Screenshot {
        output: String,
        #[serde(default)]
        full_page: bool,
        #[serde(default)]
        omit_background: bool,
    },
}

impl From<AutomationStep> for WebAutomation {
    fn from(step: AutomationStep) -> Self {
        match step {
            AutomationStep::Evaluate { script } => WebAutomation::Evaluate(script),
            AutomationStep::Click { selector } => WebAutomation::Click(selector),
            AutomationStep::ClickAll { selector } => WebAutomation::ClickAll(selector),
            AutomationStep::WaitFor { selector } => WebAutomation::WaitFor(selector),
            AutomationStep::WaitForAndClick { selector } => {
                WebAutomation::WaitForAndClick(selector)
            }
            AutomationStep::Wait { ms } => WebAutomation::Wait(ms),
            AutomationStep::WaitForNavigation => WebAutomation::WaitForNavigation,
            AutomationStep::ScrollX { pixels } => WebAutomation::ScrollX(pixels),
            AutomationStep::ScrollY { pixels } => WebAutomation::ScrollY(pixels),
            AutomationStep::InfiniteScroll { times } => WebAutomation::InfiniteScroll(times),
            AutomationStep::Fill { selector, value } => WebAutomation::Fill { selector, value },
            AutomationStep::Screenshot {
                output,
                full_page,
                omit_background,
            } => WebAutomation::Screenshot {
                full_page,
                omit_background,
                output,
            },
        }
    }
}

/// Parse an automation-scripts JSON document into spider's `AutomationScriptsMap`.
///
/// Separated from the file read so it can be unit-tested without disk I/O.
pub fn parse_automation_scripts(json: &str) -> Result<AutomationScriptsMap, Box<dyn Error>> {
    let raw: HashMap<String, Vec<AutomationStep>> =
        serde_json::from_str(json).map_err(|e| format!("invalid automation-script JSON: {e}"))?;
    if raw.is_empty() {
        return Err("automation-script JSON defines no path prefixes".into());
    }
    let mut map = AutomationScriptsMap::new();
    for (prefix, steps) in raw {
        // Keys are URL *path* prefixes that spider matches against each page's
        // path. A key without a leading '/' (e.g. "blog") can never match and
        // would silently become a no-op, so reject it up front.
        if !prefix.starts_with('/') {
            return Err(format!(
                "automation-script prefix {prefix:?} must be a URL path starting with '/'"
            )
            .into());
        }
        if steps.is_empty() {
            return Err(format!("automation-script prefix {prefix:?} has no steps").into());
        }
        map.insert(prefix, steps.into_iter().map(WebAutomation::from).collect());
    }
    Ok(map)
}

/// Load and parse the automation-scripts file at `path`.
pub async fn load_automation_scripts(path: &Path) -> Result<AutomationScriptsMap, Box<dyn Error>> {
    let json = tokio::fs::read_to_string(path).await.map_err(|e| {
        format!(
            "failed to read automation-script file {}: {e}",
            path.display()
        )
    })?;
    parse_automation_scripts(&json)
}

#[cfg(test)]
#[path = "automation_tests.rs"]
mod tests;
