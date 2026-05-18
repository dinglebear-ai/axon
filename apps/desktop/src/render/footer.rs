use gpui::{
    FontWeight, IntoElement, MouseButton, MouseDownEvent, ParentElement, SharedString, Styled, div,
    prelude::*, px, rgb,
};

use super::pulsing_dot;
use crate::ClearOutput;
use crate::actions::CommandAction;
use crate::output::{CommandOutput, OutputKind};
use crate::theme::{
    AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG, AURORA_CONTROL_SURFACE, AURORA_FONT_MONO,
    AURORA_NAV_BG, AURORA_TEXT_MUTED, AURORA_TEXT_PRIMARY,
};
use crate::ui::RunningCommand;

pub(crate) fn render_palette_footer(
    action: CommandAction,
    output: Option<&CommandOutput>,
    running: Option<&RunningCommand>,
    conversation_hint: Option<SharedString>,
) -> impl IntoElement {
    let model = FooterModel::new(action, output, running);

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
            model.accent,
            px(7.0),
            model.is_running,
        ))
        .child(render_status_group(
            model.label,
            model.elapsed_label,
            model.accent,
        ))
        .child(render_text_group(model.title, model.detail))
        .child(render_conversation_slot(conversation_hint))
        .when(model.has_output, |el| el.child(render_clear_button()))
}

struct FooterModel<'a> {
    accent: u32,
    label: SharedString,
    title: String,
    detail: &'a str,
    elapsed_label: Option<String>,
    has_output: bool,
    is_running: bool,
}

impl<'a> FooterModel<'a> {
    fn new(
        action: CommandAction,
        output: Option<&'a CommandOutput>,
        running: Option<&RunningCommand>,
    ) -> Self {
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

        let running_title = running.map(|r| format!("Running {}…", r.label));
        let title = running_title
            .or_else(|| output.map(|o| o.title.clone()))
            .unwrap_or_else(|| action.description.to_string());

        Self {
            accent,
            label: SharedString::from(label),
            title,
            detail: output
                .map(|o| o.subtitle.as_str())
                .unwrap_or(action.example),
            elapsed_label: running.map(|r| r.elapsed_label()),
            has_output: output.is_some() && !is_running,
            is_running,
        }
    }
}

fn render_status_group(
    label: SharedString,
    elapsed_label: Option<String>,
    accent: u32,
) -> impl IntoElement {
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
        })
}

fn render_text_group(title: String, detail: &str) -> impl IntoElement {
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
                .child(SharedString::from(title)),
        )
        .child(
            div()
                .font_family(AURORA_FONT_MONO)
                .font_weight(FontWeight(480.0))
                .text_size(px(11.0))
                .text_color(rgb(AURORA_TEXT_MUTED))
                .child(SharedString::from(detail.to_string())),
        )
}

fn render_conversation_slot(conversation_hint: Option<SharedString>) -> impl IntoElement {
    div()
        .w(px(180.0))
        .px_2()
        .font_family(AURORA_FONT_MONO)
        .font_weight(FontWeight(500.0))
        .text_size(px(11.0))
        .text_color(rgb(AURORA_TEXT_MUTED))
        .child(conversation_hint.unwrap_or_else(|| SharedString::from("")))
}

fn render_clear_button() -> impl IntoElement {
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
        .child("clear ✕")
}
