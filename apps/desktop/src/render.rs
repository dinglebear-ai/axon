use std::time::Duration;

mod action_rows;
mod output_body;
pub(crate) use action_rows::render_action_rows_interactive;
pub(crate) use output_body::render_output_body;

use gpui::{
    Animation, AnimationExt, ElementId, FocusHandle, FontWeight, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, SharedString, Styled, div, prelude::*, pulsating_between, px,
    rgb,
};

use crate::actions::CommandAction;
use crate::output::{CommandOutput, OutputKind};
use crate::theme::{
    AURORA_ACCENT_STRONG, AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG, AURORA_CONTROL_SURFACE,
    AURORA_FONT_DISPLAY, AURORA_FONT_MONO, AURORA_HOVER_BG, AURORA_NAV_BG, AURORA_TEXT_MUTED,
    AURORA_TEXT_PRIMARY,
};
use crate::ui::RunningCommand;
use crate::{ClearOutput, Submit, ToggleActionMenu};

pub(crate) fn render_prompt_row(
    query_is_empty: bool,
    locked_command: Option<CommandAction>,
    prompt: SharedString,
    input_active: bool,
    focus_handle: FocusHandle,
    action_menu_open: bool,
    status_dot: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .h(px(48.0))
        .px_4()
        .border_b_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .cursor_text()
        .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
            window.focus(&focus_handle, cx);
        })
        .child(status_dot)
        .child(
            div()
                .cursor_pointer()
                .rounded_sm()
                .px_1()
                .py_1()
                .hover(|el| el.bg(rgb(AURORA_NAV_BG)))
                .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, window, cx| {
                    window.dispatch_action(Box::new(ToggleActionMenu), cx);
                })
                .font_family(AURORA_FONT_DISPLAY)
                .font_weight(FontWeight(760.0))
                .text_size(px(15.0))
                .text_color(if action_menu_open {
                    rgb(AURORA_ACCENT_STRONG)
                } else {
                    rgb(AURORA_TEXT_PRIMARY)
                })
                .child("axon ▾"),
        )
        .when_some(locked_command, |el, action| {
            el.child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .bg(rgb(AURORA_NAV_BG))
                    .border_1()
                    .border_color(rgb(AURORA_BORDER_STRONG))
                    .font_family(AURORA_FONT_MONO)
                    .font_weight(FontWeight(650.0))
                    .text_size(px(11.0))
                    .text_color(rgb(AURORA_ACCENT_STRONG))
                    .child(action.subcommand),
            )
        })
        .child(
            div()
                .flex_1()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .font_weight(FontWeight(480.0))
                        .text_size(px(14.0))
                        .text_color(if query_is_empty {
                            rgb(AURORA_TEXT_MUTED)
                        } else {
                            rgb(AURORA_TEXT_PRIMARY)
                        })
                        .child(prompt),
                )
                .when(input_active, |el| {
                    el.child(
                        div()
                            .id("prompt-caret")
                            .w(px(1.5))
                            .h(px(18.0))
                            .rounded_sm()
                            .bg(rgb(AURORA_ACCENT_STRONG))
                            .with_animation(
                                "prompt-caret-blink",
                                Animation::new(Duration::from_millis(1000))
                                    .repeat()
                                    .with_easing(pulsating_between(0.12, 1.0)),
                                |el, delta| el.opacity(delta),
                            ),
                    )
                }),
        )
        .child(
            div()
                .cursor_pointer()
                .rounded_sm()
                .px_2()
                .py_1()
                .border_1()
                .border_color(rgb(AURORA_BORDER_STRONG))
                .bg(rgb(AURORA_NAV_BG))
                .hover(|el| el.bg(rgb(AURORA_HOVER_BG)))
                .font_family(AURORA_FONT_MONO)
                .font_weight(FontWeight(700.0))
                .text_size(px(12.0))
                .text_color(rgb(AURORA_ACCENT_STRONG))
                .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, window, cx| {
                    window.dispatch_action(Box::new(Submit), cx);
                })
                .child("send"),
        )
}

pub(crate) fn render_palette_footer(
    action: CommandAction,
    output: Option<&CommandOutput>,
    running: Option<&RunningCommand>,
    conversation_hint: Option<SharedString>,
) -> impl IntoElement {
    let is_running = running.is_some();
    let status = output.map(|o| o.kind).unwrap_or(OutputKind::Warning);
    let accent = if is_running {
        OutputKind::Running.accent_color()
    } else if output.is_some() {
        status.accent_color()
    } else {
        AURORA_BORDER_STRONG
    };
    let label = if is_running {
        "running"
    } else if let Some(output) = output {
        output.kind.label()
    } else {
        "enter"
    };
    let elapsed_label = running.map(|r| r.elapsed_label());
    // Use the live "Running <label>" string while a command is in flight so the
    // footer reads as a status, not as the action's static description.
    let running_title = running.map(|r| format!("Running {}…", r.label));
    let title = running_title
        .as_deref()
        .or_else(|| output.map(|o| o.title.as_str()))
        .unwrap_or(action.description);
    let detail = output
        .map(|o| o.subtitle.as_str())
        .unwrap_or(action.example);
    let has_output = output.is_some() && !is_running;

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .px_4()
        .py_2()
        .border_t_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .bg(rgb(AURORA_CONTROL_SURFACE))
        .child(pulsing_dot(
            "footer-status-dot",
            accent,
            px(7.0),
            is_running,
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .bg(rgb(AURORA_NAV_BG))
                        .font_family(AURORA_FONT_MONO)
                        .font_weight(FontWeight(650.0))
                        .text_size(px(11.0))
                        .text_color(rgb(accent))
                        .child(label),
                )
                .when_some(elapsed_label, |el, elapsed| {
                    el.child(
                        div()
                            .min_w(px(48.0))
                            .font_family(AURORA_FONT_MONO)
                            .font_weight(FontWeight(560.0))
                            .text_size(px(11.0))
                            .text_color(rgb(AURORA_TEXT_MUTED))
                            .child(SharedString::from(elapsed)),
                    )
                }),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_px()
                .child(
                    div()
                        .font_weight(FontWeight(620.0))
                        .text_size(px(12.0))
                        .text_color(rgb(AURORA_TEXT_PRIMARY))
                        .child(SharedString::from(title.to_string())),
                )
                .child(
                    div()
                        .font_family(AURORA_FONT_MONO)
                        .font_weight(FontWeight(480.0))
                        .text_size(px(11.0))
                        .text_color(rgb(AURORA_TEXT_MUTED))
                        .child(SharedString::from(detail.to_string())),
                ),
        )
        // Conversation hint — fixed-width slot so its appearance/disappearance
        // does not shift surrounding footer elements. Empty string when no
        // ask conversation is live.
        .child(
            div()
                .w(px(180.0))
                .px_2()
                .font_family(AURORA_FONT_MONO)
                .font_weight(FontWeight(500.0))
                .text_size(px(11.0))
                .text_color(rgb(AURORA_TEXT_MUTED))
                .child(conversation_hint.unwrap_or_else(|| SharedString::from(""))),
        )
        // Dismiss button — only shown when there's output to clear.
        .when(has_output, |el| {
            el.child(
                div()
                    .cursor_pointer()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .font_family(AURORA_FONT_MONO)
                    .font_weight(FontWeight(560.0))
                    .text_size(px(11.0))
                    .text_color(rgb(AURORA_TEXT_MUTED))
                    .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, window, cx| {
                        window.dispatch_action(Box::new(ClearOutput), cx);
                    })
                    .child("clear ✕"),
            )
        })
}

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
