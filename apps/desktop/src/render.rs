use gpui::{
    FontWeight, IntoElement, ParentElement, SharedString, Styled, div, prelude::*, px, rgb,
};

use crate::actions::{ArgMode, CommandAction};
use crate::output::{CommandOutput, OutputKind, OutputSection};
use crate::theme::{
    AURORA_ACCENT_PRIMARY, AURORA_ACCENT_STRONG, AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG,
    AURORA_CONTROL_SURFACE, AURORA_FONT_DISPLAY, AURORA_FONT_MONO, AURORA_HOVER_BG, AURORA_NAV_BG,
    AURORA_OUTPUT_MUTED, AURORA_OUTPUT_TEXT, AURORA_PANEL_STRONG, AURORA_TEXT_MUTED,
    AURORA_TEXT_PRIMARY,
};

pub(crate) fn render_prompt_row(query_is_empty: bool, prompt: SharedString) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .h(px(44.0))
        .px_3()
        .border_b_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .child(
            div()
                .font_family(AURORA_FONT_DISPLAY)
                .font_weight(FontWeight(760.0))
                .text_size(px(15.0))
                .text_color(rgb(AURORA_TEXT_PRIMARY))
                .child("axon"),
        )
        .child(
            div()
                .font_family(AURORA_FONT_MONO)
                .font_weight(FontWeight(650.0))
                .text_size(px(11.0))
                .text_color(rgb(AURORA_ACCENT_STRONG))
                .child("palette"),
        )
        .child(
            div()
                .flex_1()
                .font_weight(FontWeight(560.0))
                .text_size(px(14.0))
                .text_color(if query_is_empty {
                    rgb(AURORA_TEXT_MUTED)
                } else {
                    rgb(AURORA_TEXT_PRIMARY)
                })
                .child(prompt),
        )
}

pub(crate) fn render_action_rows(
    actions: Vec<CommandAction>,
    selected: usize,
    running_subcommand: Option<&'static str>,
) -> impl IntoElement {
    let is_empty = actions.is_empty();
    div()
        .flex()
        .flex_col()
        .py_1()
        .when(is_empty, |el| el.child(render_empty_row()))
        .when(!is_empty, |el| {
            el.children(actions.into_iter().enumerate().map(move |(i, action)| {
                let is_sel = i == selected;
                let is_running = running_subcommand == Some(action.subcommand);
                render_action_row(action, is_sel, is_running)
            }))
        })
}

pub(crate) fn render_output_body(output: CommandOutput) -> impl IntoElement {
    let empty = output.stdout.is_none() && output.stderr.is_none();
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .font_weight(FontWeight(720.0))
                        .text_size(px(14.0))
                        .text_color(rgb(AURORA_TEXT_PRIMARY))
                        .child(SharedString::from(output.title)),
                )
                .child(
                    div()
                        .font_family(AURORA_FONT_MONO)
                        .font_weight(FontWeight(520.0))
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
                    .font_weight(FontWeight(560.0))
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
            el.child(render_output_section(section, OutputKind::Success))
        })
        .when_some(output.stderr, |el, section| {
            el.child(render_output_section(section, OutputKind::Error))
        })
}

pub(crate) fn render_palette_footer(
    action: CommandAction,
    output: Option<&CommandOutput>,
    is_running: bool,
) -> impl IntoElement {
    let status = output
        .map(|output| output.kind)
        .unwrap_or(OutputKind::Warning);
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
        .map(|output| output.title.as_str())
        .unwrap_or(action.description);
    let detail = output
        .map(|output| output.subtitle.as_str())
        .unwrap_or(action.example);

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .px_3()
        .py_2()
        .border_t_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .bg(rgb(AURORA_CONTROL_SURFACE))
        .child(div().size_2().rounded_full().bg(rgb(accent)))
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
                .gap_1()
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
                        .font_weight(FontWeight(520.0))
                        .text_size(px(11.0))
                        .text_color(rgb(AURORA_TEXT_MUTED))
                        .child(SharedString::from(detail.to_string())),
                ),
        )
}

fn render_output_section(section: OutputSection, kind: OutputKind) -> impl IntoElement {
    let accent = kind.accent_color();
    div()
        .flex()
        .flex_col()
        .rounded_sm()
        .border_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .bg(rgb(AURORA_CONTROL_SURFACE))
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
                        .font_weight(FontWeight(740.0))
                        .text_size(px(11.0))
                        .text_color(rgb(accent))
                        .child(section.label),
                )
                .child(
                    div()
                        .font_family(AURORA_FONT_MONO)
                        .font_weight(FontWeight(560.0))
                        .text_size(px(11.0))
                        .text_color(rgb(AURORA_OUTPUT_MUTED))
                        .child(SharedString::from(format!("{} lines", section.line_count))),
                ),
        )
        .child(div().flex().flex_col().gap_1().px_3().py_3().children(
            section.text.lines().take(220).map(|line| {
                div()
                    .font_family(AURORA_FONT_MONO)
                    .font_weight(FontWeight(500.0))
                    .text_size(px(12.0))
                    .line_height(px(17.0))
                    .text_color(rgb(AURORA_OUTPUT_TEXT))
                    .child(SharedString::from(line.to_string()))
            }),
        ))
}

fn render_empty_row() -> impl IntoElement {
    div()
        .h(px(34.0))
        .flex()
        .items_center()
        .px_3()
        .font_weight(FontWeight(560.0))
        .text_size(px(13.0))
        .text_color(rgb(AURORA_TEXT_MUTED))
        .child("No matching axon commands")
}

fn render_action_row(
    action: CommandAction,
    is_selected: bool,
    is_running: bool,
) -> impl IntoElement {
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

    div()
        .h(px(34.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .mx_1()
        .px_2()
        .rounded_sm()
        .bg(if is_selected {
            rgb(AURORA_HOVER_BG)
        } else {
            rgb(AURORA_PANEL_STRONG)
        })
        .child(
            div()
                .font_weight(if is_selected {
                    FontWeight(650.0)
                } else {
                    FontWeight(560.0)
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
                .font_weight(FontWeight(620.0))
                .text_size(px(11.0))
                .text_color(rgb(meta_color))
                .child(meta),
        )
}
