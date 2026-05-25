//! Aurora bordered summary panel. Use for terminal "you're done" output:
//! crawl/ingest/embed completion, doctor summary, stats overview.
//!
//! ```text
//! ╭─ Crawl complete ──╮
//! │ pages    42       │
//! │ chunks   1024     │
//! │ elapsed  12.3s    │
//! ╰───────────────────╯
//! ```
//!
//! Layout invariant: every rendered line has identical visible width
//! `inner_w + 2` (the `+2` is the two `│`/`╭`/`╮` border glyphs). Tests assert
//! this; do not change row padding without keeping the top/bottom math in
//! sync.

use crate::core::ui::{ACCENT_ANSI, PRIMARY_ANSI, ansi_colorize, color_enabled_public, muted};

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;

/// Render a titled panel with key/value rows. Honors `--color` via
/// `color_enabled_public()`.
pub fn panel(title: &str, rows: &[(&str, &str)]) -> String {
    render(title, rows, color_enabled_public())
}

/// ANSI-free variant — used by tests. Production `--color=never` flows through
/// `panel()` and short-circuits via `color_enabled_public()`; this helper is
/// purely a test seam.
#[cfg(test)]
pub(crate) fn panel_plain(title: &str, rows: &[(&str, &str)]) -> String {
    render(title, rows, false)
}

fn render(title: &str, rows: &[(&str, &str)], color: bool) -> String {
    let title_chars = title.chars().count();
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
    // body visible width inside `│ ... │` when written as " key  value "
    // — i.e. the row's content area between the two border glyphs:
    //   1 leading space + key_w + 2 separator + val_w + 1 trailing space
    let row_visible = if rows.is_empty() {
        0
    } else {
        1 + key_w + 2 + val_w + 1
    };
    // top visible width inside `╭ ... ╮` when written as "─ title ─...─":
    //   1 leading ─ + 1 space + title + 1 space + N dashes
    // Need N ≥ 0, so the inner visible width must be ≥ title_chars + 3.
    let inner_w = row_visible.max(title_chars + 3);
    let dashes_after_title = inner_w - title_chars - 3;
    let extra_row_pad = inner_w.saturating_sub(row_visible);

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

    // Top: ╭─ title ─...─╮
    out.push_str(&border("╭"));
    out.push_str(&border("─"));
    out.push(' ');
    out.push_str(&title_styled);
    out.push(' ');
    out.push_str(&dashes(dashes_after_title));
    out.push_str(&border("╮"));
    out.push('\n');

    // Rows: │ key  value <extra-pad-when-title-wider>│
    for (k, v) in rows {
        out.push_str(&border("│"));
        out.push(' ');
        out.push_str(&key_styled(k));
        out.push_str(&" ".repeat(key_w - k.chars().count()));
        out.push_str("  ");
        out.push_str(v);
        out.push_str(&" ".repeat(val_w - v.chars().count()));
        out.push(' ');
        out.push_str(&" ".repeat(extra_row_pad));
        out.push_str(&border("│"));
        out.push('\n');
    }

    // Bottom: ╰───╯ — same dash count as the top's full inner width.
    out.push_str(&border("╰"));
    out.push_str(&dashes(inner_w));
    out.push_str(&border("╯"));
    out
}
