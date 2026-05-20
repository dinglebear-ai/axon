//! Pure, side-effect-free window-height computation for the palette.
//!
//! The palette window is **content-driven**: on launch we render only the
//! prompt row (≈ input + thin border), and the window grows as the user
//! types, runs a command, and sees output. This module computes the target
//! window height from a snapshot of the palette state. `ui.rs` calls it on
//! every render to drive a one-shot resize tick.
//!
//! Keeping it pure makes it trivially testable and ensures the launch path
//! never spawns a background task — `compute_desired_height` runs inside
//! `render()`.

/// Snapshot of the palette state that affects window height.
///
/// Built from the live `Palette` inside `render()`. Plain numbers /
/// booleans so the function is straightforward to test.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HeightSnapshot {
    /// Number of action rows that will be rendered (matches list visible).
    pub(crate) action_row_count: usize,
    /// True when the action list is visible but has zero matches — the
    /// "No matching commands" placeholder row is rendered in that case.
    pub(crate) empty_placeholder_visible: bool,
    /// True when a selected action's footer slot is rendered.
    pub(crate) footer_visible: bool,
    /// True when the output card is rendered with a body (stdout/stderr).
    pub(crate) output_body_visible: bool,
    /// True when the settings editor is rendered.
    pub(crate) settings_visible: bool,
}

// ── per-component pixel budgets ───────────────────────────────────────────
//
// These mirror the literals in `render.rs` and the chrome in `ui.rs`. They
// are intentionally co-located here so the height compute is a single
// source of truth; the render functions still own the actual layout.

/// Outer page padding (`p_5()` in `ui.rs`) — 20px top + 20px bottom.
const PAGE_PADDING_V: f32 = 40.0;

/// Border around the centered card (1px top + 1px bottom).
const CARD_BORDER_V: f32 = 2.0;

/// Prompt row (`h(px(48.0))` in `render::render_prompt_row`) plus its 1px
/// bottom border.
const PROMPT_ROW: f32 = 49.0;

/// Single action row (`h(px(38.0))`) plus 4px breathing room from the
/// row container's `py_1()`.
const ACTION_ROW: f32 = 38.0;

/// Vertical padding around the action list container (`py_1()` = 4px top +
/// 4px bottom).
const ACTION_LIST_PADDING: f32 = 8.0;

/// Footer row (label/status + dot) — `py_2()` (8+8) + ~2 lines of 12px text
/// at line-height 1.4 (~34px). Conservative measure.
const FOOTER_ROW: f32 = 52.0;

/// Output card with body in the auto-sized palette. Manual user resizes can
/// make the rendered output region taller; this value is only the automatic
/// height budget.
const OUTPUT_BODY: f32 = 320.0;

/// Minimum window height — "prompt only" state. This is the launch height.
pub(crate) const MIN_WINDOW_HEIGHT: f32 = PAGE_PADDING_V + CARD_BORDER_V + PROMPT_ROW;

/// Custom Zed-style title/menu strip drawn inside the window. On Windows the
/// native titlebar is hidden, so the outer window needs to reserve this height.
pub(crate) const WINDOW_CHROME_HEIGHT: f32 = 34.0;

/// Maximum window height — even with everything visible, cap so the
/// palette doesn't fill the whole display. `720px` is the chosen upper
/// bound for "everything open" on typical desktop displays.
pub(crate) const MAX_WINDOW_HEIGHT: f32 = 720.0;

/// Compute the target window height from a state snapshot.
pub(crate) fn compute_desired_height(snap: HeightSnapshot) -> f32 {
    let mut h = MIN_WINDOW_HEIGHT;

    if snap.settings_visible {
        return MAX_WINDOW_HEIGHT;
    }

    if snap.action_row_count > 0 {
        h += ACTION_LIST_PADDING + (snap.action_row_count as f32) * ACTION_ROW;
    } else if snap.empty_placeholder_visible {
        // Even with zero matches, `render_action_rows` still renders the
        // "No matching commands" placeholder row. Reserve one row of
        // height so the placeholder is visible instead of clipped.
        h += ACTION_LIST_PADDING + ACTION_ROW;
    }

    if snap.footer_visible {
        h += FOOTER_ROW;
    }

    if snap.output_body_visible {
        h += OUTPUT_BODY;
    }

    h.min(MAX_WINDOW_HEIGHT)
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
