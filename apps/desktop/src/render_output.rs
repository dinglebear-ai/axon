use gpui::{FontWeight, IntoElement, ParentElement, SharedString, Styled, div, prelude::*, px, rgb};

use crate::markdown::render_markdown;
use crate::output::{CommandOutput, OutputKind, OutputSection};
use crate::theme::{
    AURORA_BORDER_DEFAULT, AURORA_CONTROL_SURFACE, AURORA_FONT_MONO, AURORA_OUTPUT_MUTED,
    AURORA_OUTPUT_TEXT, AURORA_TEXT_MUTED, AURORA_TEXT_PRIMARY,
};

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
