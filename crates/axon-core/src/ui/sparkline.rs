//! Unicode sparkline renderer — one char per data point at 8 levels.
//!
//! Intended for future inline trend displays in stats-style output.

use crate::ui::{ACCENT_ANSI, ansi_colorize, color_enabled_public};

#[cfg(test)]
#[path = "sparkline_tests.rs"]
mod tests;

const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Render `values` as a sparkline. Empty input → empty string. Returns Aurora
/// cyan text when color is enabled.
pub fn sparkline(values: &[u64]) -> String {
    if color_enabled_public() {
        ansi_colorize(ACCENT_ANSI, &sparkline_plain(values))
    } else {
        sparkline_plain(values)
    }
}

pub(crate) fn sparkline_plain(values: &[u64]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let min = *values.iter().min().unwrap();
    let max = *values.iter().max().unwrap();
    if min == max {
        // All equal: render a flat mid-level line so the user still sees
        // something rather than an invisible streak of `▁`.
        return BLOCKS[3].to_string().repeat(values.len());
    }
    let range = (max - min) as f64;
    values
        .iter()
        .map(|&v| {
            let normalized = ((v - min) as f64) / range;
            let idx =
                ((normalized * (BLOCKS.len() - 1) as f64).round() as usize).min(BLOCKS.len() - 1);
            BLOCKS[idx]
        })
        .collect()
}
