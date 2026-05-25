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
    if color_enabled_public() {
        panel_colored(title, rows)
    } else {
        panel_plain(title, rows)
    }
}

/// ANSI-free variant — used by tests and `--color=never`.
pub(crate) fn panel_plain(title: &str, rows: &[(&str, &str)]) -> String {
    let (key_w, val_w, inner_w) = layout(title, rows);
    let mut out = String::new();
    push_top_border(&mut out, title, inner_w, |s, t| s.push_str(t));
    for (k, v) in rows {
        push_row(
            &mut out,
            k,
            v,
            key_w,
            val_w,
            |s, t| s.push_str(t),
            |s, t| s.push_str(t),
        );
    }
    push_bottom_border(&mut out, inner_w, |s, t| s.push_str(t));
    out
}

fn panel_colored(title: &str, rows: &[(&str, &str)]) -> String {
    let (key_w, val_w, inner_w) = layout(title, rows);
    let border = |ch: char| ansi_colorize(ACCENT_ANSI, &ch.to_string());
    let mut out = String::new();

    out.push_str(&border('╭'));
    out.push_str(&border('─'));
    out.push(' ');
    out.push_str(&ansi_colorize(PRIMARY_ANSI, title));
    out.push(' ');
    for _ in (title.chars().count() + 4)..(inner_w + 2) {
        out.push_str(&border('─'));
    }
    out.push_str(&border('╮'));
    out.push('\n');

    for (k, v) in rows {
        out.push_str(&border('│'));
        out.push(' ');
        out.push_str(&muted(k));
        for _ in k.chars().count()..key_w {
            out.push(' ');
        }
        out.push_str("  ");
        out.push_str(v);
        for _ in v.chars().count()..val_w {
            out.push(' ');
        }
        out.push(' ');
        out.push_str(&border('│'));
        out.push('\n');
    }

    out.push_str(&border('╰'));
    for _ in 0..(inner_w + 2) {
        out.push_str(&border('─'));
    }
    out.push_str(&border('╯'));
    out
}

fn layout(title: &str, rows: &[(&str, &str)]) -> (usize, usize, usize) {
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
    // Inner width must hold "key  value" (separator = 2 spaces) AND the title.
    let body_w = if rows.is_empty() {
        0
    } else {
        key_w + 2 + val_w
    };
    let inner_w = body_w.max(title.chars().count() + 2);
    (key_w, val_w, inner_w)
}

fn push_top_border<F: Fn(&mut String, &str)>(
    out: &mut String,
    title: &str,
    inner_w: usize,
    paint: F,
) {
    paint(out, "╭");
    paint(out, "─");
    out.push(' ');
    out.push_str(title);
    out.push(' ');
    for _ in (title.chars().count() + 4)..(inner_w + 2) {
        paint(out, "─");
    }
    paint(out, "╮");
    out.push('\n');
}

fn push_row<F1, F2>(
    out: &mut String,
    k: &str,
    v: &str,
    key_w: usize,
    val_w: usize,
    paint_border: F1,
    paint_key: F2,
) where
    F1: Fn(&mut String, &str),
    F2: Fn(&mut String, &str),
{
    paint_border(out, "│");
    out.push(' ');
    paint_key(out, k);
    for _ in k.chars().count()..key_w {
        out.push(' ');
    }
    out.push_str("  ");
    out.push_str(v);
    for _ in v.chars().count()..val_w {
        out.push(' ');
    }
    out.push(' ');
    paint_border(out, "│");
    out.push('\n');
}

fn push_bottom_border<F: Fn(&mut String, &str)>(out: &mut String, inner_w: usize, paint: F) {
    paint(out, "╰");
    for _ in 0..(inner_w + 2) {
        paint(out, "─");
    }
    paint(out, "╯");
}
