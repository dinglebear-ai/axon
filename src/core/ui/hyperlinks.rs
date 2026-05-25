//! OSC 8 hyperlink emitter. Modern terminals (kitty, iTerm2, wezterm, vscode,
//! Windows Terminal, gnome-terminal 3.26+) recognize the sequence and render
//! the label as a clickable link to `url`. Unsupported terminals just print
//! the label as plain text.
//!
//! Format: `\x1b]8;;URL\x1b\\TEXT\x1b]8;;\x1b\\`

use crate::core::ui::color_enabled_public;

#[cfg(test)]
#[path = "hyperlinks_tests.rs"]
mod tests;

const OSC8: &str = "\x1b]8;;";
const ST: &str = "\x1b\\";

/// Render `label` as a clickable link to `url` if the terminal supports OSC 8
/// AND color output is enabled (so `--color=never` also strips hyperlinks).
/// Otherwise return `label` (or `url` when `label` is empty).
pub fn hyperlink(url: &str, label: &str) -> String {
    let supported =
        color_enabled_public() && supports_hyperlinks::on(supports_hyperlinks::Stream::Stdout);
    hyperlink_for_test(url, label, supported)
}

/// Test seam — caller forces the support flag.
pub(crate) fn hyperlink_for_test(url: &str, label: &str, supported: bool) -> String {
    let visible = if label.is_empty() { url } else { label };
    if !supported {
        return visible.to_string();
    }
    format!("{OSC8}{url}{ST}{visible}{OSC8}{ST}")
}
