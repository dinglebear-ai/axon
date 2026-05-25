//! Aurora-themed table renderer. Wraps `comfy-table` with the cyan accent
//! border preset, muted header style, and a `--color=never`-friendly fallback.

use crate::core::ui::color_enabled_public;
use comfy_table::{Cell, Color, ContentArrangement, Table, modifiers, presets};

#[cfg(test)]
#[path = "table_tests.rs"]
mod tests;

/// Build a table pre-styled with Aurora colors. Caller fills headers + rows.
///
/// ```ignore
/// let mut t = aurora_table(&["URL", "Chunks"]);
/// t.add_row(vec!["https://example.com".to_string(), "42".to_string()]);
/// println!("{t}");
/// ```
pub fn aurora_table(headers: &[&str]) -> Table {
    let mut t = Table::new();
    if color_enabled_public() {
        t.load_preset(presets::UTF8_FULL)
            .apply_modifier(modifiers::UTF8_ROUND_CORNERS)
            .set_content_arrangement(ContentArrangement::Dynamic);
        let cyan = Color::Rgb {
            r: 41,
            g: 182,
            b: 246,
        };
        t.set_header(headers.iter().map(|h| Cell::new(h).fg(cyan)));
    } else {
        t.load_preset(presets::ASCII_FULL_CONDENSED);
        t.set_header(headers.to_vec());
    }
    t
}
