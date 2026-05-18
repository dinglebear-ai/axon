use std::time::Duration;

mod action_rows;
mod footer;
mod output_body;
mod prompt;
pub(crate) use action_rows::render_action_rows_interactive;
pub(crate) use footer::render_palette_footer;
pub(crate) use output_body::render_output_body;
pub(crate) use prompt::render_prompt_row;

use gpui::{Animation, AnimationExt, ElementId, Styled, div, prelude::*, pulsating_between, rgb};

// ── private helpers ───────────────────────────────────────────────────────────

/// A small circular dot. When `animate` is true the dot pulses its opacity
/// between 0.35 and 1.0 — the user's "the app is alive and waiting on
/// something" signal. `id` must be unique across all pulsing dots that may be
/// rendered simultaneously (GPUI keys animation state by `ElementId`).
fn pulsing_dot(
    id: impl Into<ElementId>,
    color: u32,
    size: gpui::Pixels,
    animate: bool,
) -> gpui::AnyElement {
    let base = div()
        .size(size)
        .rounded_full()
        .flex_shrink_0()
        .bg(rgb(color));
    if animate {
        base.with_animation(
            id,
            Animation::new(Duration::from_millis(1100))
                .repeat()
                .with_easing(pulsating_between(0.35, 1.0)),
            |el, delta| el.opacity(delta),
        )
        .into_any_element()
    } else {
        base.into_any_element()
    }
}
