use std::time::Duration;

mod action_rows;
pub(crate) use action_rows::render_action_rows_interactive;

use gpui::{
    Animation, AnimationExt, ElementId, FontWeight, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, SharedString, Styled, div, prelude::*, pulsating_between, px, rgb,
};

use crate::ClearOutput;
use crate::actions::CommandAction;
use crate::markdown::render_markdown;
use crate::output::{CommandOutput, OutputKind, OutputSection};
use crate::theme::{
    AURORA_ACCENT_PRIMARY, AURORA_ACCENT_STRONG, AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG,
    AURORA_CONTROL_SURFACE, AURORA_FONT_DISPLAY, AURORA_FONT_MONO, AURORA_NAV_BG,
    AURORA_OUTPUT_MUTED, AURORA_OUTPUT_TEXT, AURORA_TEXT_MUTED, AURORA_TEXT_PRIMARY,
};
use crate::ui::RunningCommand;

pub(crate) fn render_prompt_row(
    query_is_empty: bool,
    locked_command: Option<CommandAction>,
    prompt: SharedString,
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
        .child(status_dot)
        .child(
            div()
                .font_family(AURORA_FONT_DISPLAY)
                .font_weight(FontWeight(760.0))
                .text_size(px(15.0))
                .text_color(rgb(AURORA_TEXT_PRIMARY))
                .child("axon"),
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
                .font_weight(FontWeight(480.0))
                .text_size(px(14.0))
                .text_color(if query_is_empty {
                    rgb(AURORA_TEXT_MUTED)
                } else {
                    rgb(AURORA_TEXT_PRIMARY)
                })
                .child(prompt),
        )
}

pub(crate) fn render_output_body(output: CommandOutput) -> impl IntoElement {
    let empty = output.stdout.is_none() && output.stderr.is_none();
    let title_accent = output.kind.accent_color();
    let is_running = output.kind == OutputKind::Running;
    let show_subtitle = is_running || output.kind == OutputKind::Error;
    let compact_stdout = output.compact_stdout;
    div()
        .flex()
        .flex_col()
        .gap_2()
        .px_4()
        .py_2()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_px()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(pulsing_dot(
                            "output-title-dot",
                            title_accent,
                            px(7.0),
                            is_running,
                        ))
                        .child(
                            div()
                                .font_weight(FontWeight(680.0))
                                .text_size(px(13.0))
                                .text_color(rgb(AURORA_TEXT_PRIMARY))
                                .child(SharedString::from(output.title)),
                        ),
                )
                .when(show_subtitle, |el| {
                    el.child(
                        div()
                            .font_family(AURORA_FONT_MONO)
                            .font_weight(FontWeight(500.0))
                            .text_size(px(11.0))
                            .text_color(rgb(AURORA_OUTPUT_MUTED))
                            .child(SharedString::from(output.subtitle)),
                    )
                }),
        )
        .when(empty, |el| {
            el.child(
                div()
                    .px_3()
                    .py_3()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(AURORA_BORDER_DEFAULT))
                    .bg(rgb(AURORA_CONTROL_SURFACE))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .when(is_running, |el| {
                        el.child(pulsing_dot(
                            "output-body-dot",
                            AURORA_ACCENT_PRIMARY,
                            px(6.0),
                            true,
                        ))
                    })
                    .child(
                        div()
                            .font_weight(FontWeight(480.0))
                            .text_size(px(12.0))
                            .text_color(rgb(AURORA_TEXT_MUTED))
                            .child(if is_running {
                                "Working — waiting for axon to return output."
                            } else {
                                "No stdout or stderr was emitted."
                            }),
                    ),
            )
        })
        .when_some(output.stdout, |el, section| {
            el.child(render_output_section(
                section,
                OutputKind::Success,
                output.use_markdown,
                compact_stdout,
            ))
        })
        .when_some(output.stderr, |el, section| {
            el.child(render_output_section(
                section,
                OutputKind::Error,
                false,
                false,
            ))
        })
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

fn render_output_section(
    section: OutputSection,
    kind: OutputKind,
    use_markdown: bool,
    compact: bool,
) -> impl IntoElement {
    let accent = kind.accent_color();
    let text = section.text.clone();
    let rendered_lines = section.rendered_lines.clone();

    div()
        .flex()
        .flex_col()
        .rounded_sm()
        .when(!compact, |el| {
            el.border_1()
                .border_color(rgb(AURORA_BORDER_DEFAULT))
                .bg(rgb(AURORA_CONTROL_SURFACE))
        })
        // section header
        .when(!compact, |el| {
            el.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(AURORA_BORDER_DEFAULT))
                    .child(
                        div()
                            .font_family(AURORA_FONT_MONO)
                            .font_weight(FontWeight(650.0))
                            .text_size(px(11.0))
                            .text_color(rgb(accent))
                            .child(section.label),
                    )
                    .child(
                        div()
                            .font_family(AURORA_FONT_MONO)
                            .font_weight(FontWeight(480.0))
                            .text_size(px(11.0))
                            .text_color(rgb(AURORA_OUTPUT_MUTED))
                            .child(SharedString::from(format!("{} lines", section.line_count))),
                    ),
            )
        })
        // section body — markdown or raw monospace
        .child(
            div()
                .px_3()
                .py_2()
                .when(use_markdown, |el| el.child(render_markdown(&text)))
                .when(!use_markdown, |el| {
                    // `rendered_lines` is pre-computed at section
                    // construction time; the `SharedString` clones below
                    // are cheap (Arc bumps), so this renders without
                    // per-frame `String` allocations.
                    el.flex()
                        .flex_col()
                        .children(rendered_lines.iter().map(|line| {
                            div()
                                .w_full()
                                .font_family(AURORA_FONT_MONO)
                                .font_weight(FontWeight(480.0))
                                .text_size(px(12.0))
                                .line_height(px(20.0))
                                .text_color(rgb(AURORA_OUTPUT_TEXT))
                                .child(line.clone())
                        }))
                }),
        )
}

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
