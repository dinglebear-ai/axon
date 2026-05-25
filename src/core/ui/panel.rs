//! Aurora bordered summary panel. Use for terminal "you're done" output:
//! crawl/ingest/embed completion, doctor summary, stats overview.
//!
//! ```text
//! ╭─ Crawl complete ──────────╮
//! │ pages      42             │
//! │ chunks     1024           │
//! │ elapsed    12.3s          │
//! ╰───────────────────────────╯
//! ```

use crate::core::ui::{ACCENT_ANSI, PRIMARY_ANSI, ansi_colorize, color_enabled_public, muted};

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;

/// Render a titled panel with key/value rows. Honors `--color` via
/// `color_enabled_public()`.
pub fn panel(title: &str, rows: &[(&str, &str)]) -> String {
    render(title, rows, color_enabled_public())
}

/// ANSI-free variant — used by tests and `--color=never`.
#[cfg(test)]
pub(crate) fn panel_plain(title: &str, rows: &[(&str, &str)]) -> String {
    render(title, rows, false)
}

fn render(title: &str, rows: &[(&str, &str)], color: bool) -> String {
    let key_w = rows
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0);
    let val_w = rows
        .iter()
        .map(|(_, v)| v.chars().count())
        .max()
        .unwrap_or(0);
    let body_w = if rows.is_empty() {
        0
    } else {
        key_w + 2 + val_w
    };
    let inner_w = body_w.max(title.chars().count() + 2);

    let border = |s: &str| -> String {
        if color {
            ansi_colorize(ACCENT_ANSI, s)
        } else {
            s.to_string()
        }
    };
    let title_styled = if color {
        ansi_colorize(PRIMARY_ANSI, title)
    } else {
        title.to_string()
    };
    let key_styled = |k: &str| if color { muted(k) } else { k.to_string() };
    let dashes = |n: usize| border("─").repeat(n);

    let mut out = String::new();

    // Top border: ╭─ title ─...─╮
    out.push_str(&border("╭"));
    out.push_str(&border("─"));
    out.push(' ');
    out.push_str(&title_styled);
    out.push(' ');
    let title_consumed = title.chars().count() + 4;
    out.push_str(&dashes((inner_w + 2).saturating_sub(title_consumed)));
    out.push_str(&border("╮"));
    out.push('\n');

    // Rows: │ key  value │
    for (k, v) in rows {
        out.push_str(&border("│"));
        out.push(' ');
        out.push_str(&key_styled(k));
        out.push_str(&" ".repeat(key_w - k.chars().count()));
        out.push_str("  ");
        out.push_str(v);
        out.push_str(&" ".repeat(val_w - v.chars().count()));
        out.push(' ');
        out.push_str(&border("│"));
        out.push('\n');
    }

    // Bottom border: ╰───╯
    out.push_str(&border("╰"));
    out.push_str(&dashes(inner_w + 2));
    out.push_str(&border("╯"));
    out
}
