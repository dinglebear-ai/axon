use gpui::{
    AnyElement, App, Div, FontWeight, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    SharedString, Stateful, Styled, Window, div, prelude::*, px, rgb,
};

use crate::ClearOutput;
use crate::actions::{ArgMode, CommandAction};
use crate::markdown::render_markdown;
use crate::output::{CommandOutput, OutputKind, OutputSection};
use crate::theme::{
    AURORA_ACCENT_PRIMARY, AURORA_ACCENT_STRONG, AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG,
    AURORA_CONTROL_SURFACE, AURORA_FONT_DISPLAY, AURORA_FONT_MONO, AURORA_HOVER_BG, AURORA_NAV_BG,
    AURORA_OUTPUT_MUTED, AURORA_OUTPUT_TEXT, AURORA_PANEL_STRONG, AURORA_PRESSED_BG,
    AURORA_ROW_HOVER_BG, AURORA_TEXT_MUTED, AURORA_TEXT_PRIMARY,
};

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
    div()
        .flex()
        .flex_col()
        .gap_3()
        .px_4()
        .py_3()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .size(px(7.0))
                                .rounded_full()
                                .flex_shrink_0()
                                .bg(rgb(title_accent)),
                        )
                        .child(
                            div()
                                .font_weight(FontWeight(680.0))
                                .text_size(px(13.0))
                                .text_color(rgb(AURORA_TEXT_PRIMARY))
                                .child(SharedString::from(output.title)),
                        ),
                )
                .child(
                    div()
                        .font_family(AURORA_FONT_MONO)
                        .font_weight(FontWeight(500.0))
                        .text_size(px(11.0))
                        .text_color(rgb(AURORA_OUTPUT_MUTED))
                        .child(SharedString::from(output.subtitle)),
                ),
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
                    .font_weight(FontWeight(480.0))
                    .text_size(px(12.0))
                    .text_color(rgb(AURORA_TEXT_MUTED))
                    .child(if output.kind == OutputKind::Running {
                        "Waiting for axon to return output."
                    } else {
                        "No stdout or stderr was emitted."
                    }),
            )
        })
        .when_some(output.stdout, |el, section| {
            el.child(render_output_section(
                section,
                OutputKind::Success,
                output.use_markdown,
            ))
        })
        .when_some(output.stderr, |el, section| {
            el.child(render_output_section(section, OutputKind::Error, false))
        })
}

pub(crate) fn render_palette_footer(
    action: CommandAction,
    output: Option<&CommandOutput>,
    is_running: bool,
) -> impl IntoElement {
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
    let title = output
        .map(|o| o.title.as_str())
        .unwrap_or(action.description);
    let detail = output
        .map(|o| o.subtitle.as_str())
        .unwrap_or(action.example);
    let has_output = output.is_some();

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
        .child(
            div()
                .size(px(7.0))
                .rounded_full()
                .flex_shrink_0()
                .bg(rgb(accent)),
        )
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
) -> impl IntoElement {
    let accent = kind.accent_color();
    let text = section.text.clone();

    div()
        .flex()
        .flex_col()
        .rounded_sm()
        .border_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .bg(rgb(AURORA_CONTROL_SURFACE))
        // section header
        .child(
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
        // section body — markdown or raw monospace
        .child(
            div()
                .px_3()
                .py_3()
                .when(use_markdown, |el| el.child(render_markdown(&text)))
                .when(!use_markdown, |el| {
                    el.flex()
                        .flex_col()
                        .children(text.lines().take(220).map(|line| {
                            let display = if line.is_empty() {
                                SharedString::from(" ")
                            } else {
                                SharedString::from(line.to_string())
                            };
                            div()
                                .w_full()
                                .font_family(AURORA_FONT_MONO)
                                .font_weight(FontWeight(480.0))
                                .text_size(px(12.0))
                                .line_height(px(20.0))
                                .text_color(rgb(AURORA_OUTPUT_TEXT))
                                .child(display)
                        }))
                }),
        )
}

/// Render the full action list (or empty-state row) with per-row click
/// handlers. `make_on_click` is invoked once per row to produce its listener
/// — typically `|i| cx.listener(move |this, _, w, cx| this.click_action(i, w, cx))`.
pub(crate) fn render_action_list<F, L>(
    actions: Vec<CommandAction>,
    selected: usize,
    running_subcommand: Option<&'static str>,
    hide_list: bool,
    mut make_on_click: F,
) -> impl IntoElement
where
    F: FnMut(usize) -> L,
    L: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    let is_empty = actions.is_empty();
    let rows: Vec<AnyElement> = actions
        .into_iter()
        .enumerate()
        .map(|(i, action)| {
            let is_running = running_subcommand == Some(action.subcommand);
            render_action_row(action, i, i == selected, is_running)
                .on_mouse_down(MouseButton::Left, make_on_click(i))
                .into_any_element()
        })
        .collect();
    div()
        .flex()
        .flex_col()
        .py_1()
        .when(is_empty && !hide_list, |el| el.child(render_empty_row()))
        .when(!is_empty, |el| el.children(rows))
}

fn render_empty_row() -> impl IntoElement {
    div()
        .h(px(36.0))
        .flex()
        .items_center()
        .px_4()
        .font_weight(FontWeight(480.0))
        .text_size(px(13.0))
        .text_color(rgb(AURORA_TEXT_MUTED))
        .child("No matching commands")
}

/// Build an interactive action row. The caller is expected to chain
/// `.on_mouse_down(MouseButton::Left, cx.listener(...))` to wire up clicks —
/// hover/active/cursor visual feedback is already applied here so every row
/// reacts to the pointer even without an active selection.
fn render_action_row(
    action: CommandAction,
    index: usize,
    is_selected: bool,
    is_running: bool,
) -> Stateful<Div> {
    let meta = if is_running {
        "running"
    } else if action.arg_mode == ArgMode::None {
        action.subcommand
    } else {
        "argument"
    };
    let meta_color = if is_running {
        AURORA_ACCENT_PRIMARY
    } else if action.arg_mode == ArgMode::None {
        AURORA_TEXT_MUTED
    } else {
        AURORA_ACCENT_STRONG
    };

    let base_bg = if is_selected {
        AURORA_HOVER_BG
    } else {
        AURORA_PANEL_STRONG
    };
    // Keep the selected row's highlight from being "dimmed" by a hover, while
    // still giving plain rows a soft hover lift.
    let hover_bg = if is_selected {
        AURORA_HOVER_BG
    } else {
        AURORA_ROW_HOVER_BG
    };

    div()
        .id(("axon-action-row", index))
        .h(px(36.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .mx_1()
        .px_3()
        .rounded_sm()
        .cursor_pointer()
        .bg(rgb(base_bg))
        .hover(move |el| el.bg(rgb(hover_bg)))
        .active(|el| el.bg(rgb(AURORA_PRESSED_BG)))
        .child(
            div()
                .font_weight(if is_selected {
                    FontWeight(620.0)
                } else {
                    FontWeight(480.0)
                })
                .text_size(px(13.0))
                .text_color(if is_selected {
                    rgb(AURORA_TEXT_PRIMARY)
                } else {
                    rgb(AURORA_TEXT_MUTED)
                })
                .child(SharedString::from(action.label)),
        )
        .child(
            div()
                .font_family(AURORA_FONT_MONO)
                .font_weight(FontWeight(560.0))
                .text_size(px(11.0))
                .text_color(rgb(meta_color))
                .child(meta),
        )
}
