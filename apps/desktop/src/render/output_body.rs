use gpui::{
    Div, FontWeight, IntoElement, MouseButton, MouseDownEvent, ParentElement, SharedString, Styled,
    div, prelude::*, px, rgb,
};

use super::pulsing_dot;
use crate::ToggleErrors;
use crate::markdown::render_markdown_doc;
use crate::output::{CommandOutput, OutputKind, OutputSection};
use crate::theme::{
    AURORA_ACCENT_PRIMARY, AURORA_ACCENT_STRONG, AURORA_BORDER_DEFAULT, AURORA_CONTROL_SURFACE,
    AURORA_FONT_MONO, AURORA_NAV_BG, AURORA_OUTPUT_MUTED, AURORA_OUTPUT_TEXT, AURORA_TEXT_MUTED,
    AURORA_TEXT_PRIMARY,
};

pub(crate) fn render_output_body(output: CommandOutput, errors_open: bool) -> impl IntoElement {
    let empty = output.stdout.is_none() && output.stderr.is_none();
    let title_accent = output.kind.accent_color();
    let is_running = output.kind == OutputKind::Running;
    let show_subtitle = is_running || output.kind == OutputKind::Error;
    let compact_stdout = output.compact_stdout;
    let has_stdout = output.stdout.is_some();
    let has_stderr = output.stderr.is_some();
    let show_errors = has_stderr && (!has_stdout || errors_open);
    let stdout = output.stdout.clone();
    let stderr = output.stderr.clone();

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
        .when(has_stdout && has_stderr, |el| {
            el.child(render_output_tabs(
                errors_open,
                stderr.as_ref().map_or(0, |s| s.line_count),
            ))
        })
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
                                "Working - waiting for axon to return output."
                            } else {
                                "No stdout or stderr was emitted."
                            }),
                    ),
            )
        })
        .when_some(stdout.filter(|_| !show_errors), |el, section| {
            el.child(render_output_section(
                section,
                OutputKind::Success,
                output.use_markdown,
                compact_stdout,
            ))
        })
        .when_some(stderr.filter(|_| show_errors), |el, section| {
            el.child(render_output_section(
                section,
                OutputKind::Error,
                false,
                false,
            ))
        })
}

fn render_output_section(
    section: OutputSection,
    kind: OutputKind,
    use_markdown: bool,
    compact: bool,
) -> impl IntoElement {
    let accent = kind.accent_color();
    let rendered_lines = section.rendered_lines.clone();
    let markdown = section.markdown.clone();

    div()
        .flex()
        .flex_col()
        .rounded_sm()
        .when(!compact, |el| {
            el.border_1()
                .border_color(rgb(AURORA_BORDER_DEFAULT))
                .bg(rgb(AURORA_CONTROL_SURFACE))
        })
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
        .child(
            div()
                .px_3()
                .py_2()
                .when_some(markdown.filter(|_| use_markdown), |el, markdown| {
                    el.child(render_markdown_doc(&markdown))
                })
                .when(!use_markdown, |el| {
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

fn render_output_tabs(errors_open: bool, error_lines: usize) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .px_3()
        .py_1()
        .rounded_sm()
        .bg(rgb(AURORA_CONTROL_SURFACE))
        .child(
            render_output_tab("stdout", !errors_open, None).on_mouse_down(
                MouseButton::Left,
                move |_: &MouseDownEvent, window, cx| {
                    if errors_open {
                        window.dispatch_action(Box::new(ToggleErrors), cx);
                    }
                },
            ),
        )
        .child(
            render_output_tab("errors", errors_open, Some(error_lines)).on_mouse_down(
                MouseButton::Left,
                |_: &MouseDownEvent, window, cx| {
                    window.dispatch_action(Box::new(ToggleErrors), cx);
                },
            ),
        )
}

fn render_output_tab(label: &'static str, active: bool, count: Option<usize>) -> Div {
    let text = count
        .map(|count| format!("{label} {count}"))
        .unwrap_or_else(|| label.to_string());

    div()
        .px_2()
        .py_1()
        .rounded_sm()
        .cursor_pointer()
        .border_1()
        .border_color(rgb(if active {
            AURORA_ACCENT_PRIMARY
        } else {
            AURORA_BORDER_DEFAULT
        }))
        .bg(rgb(if active {
            AURORA_NAV_BG
        } else {
            AURORA_CONTROL_SURFACE
        }))
        .font_family(AURORA_FONT_MONO)
        .font_weight(FontWeight(650.0))
        .text_size(px(11.0))
        .text_color(rgb(if active {
            AURORA_ACCENT_STRONG
        } else {
            AURORA_TEXT_MUTED
        }))
        .child(SharedString::from(text))
}
