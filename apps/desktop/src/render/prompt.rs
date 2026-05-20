use std::time::Duration;

use gpui::{
    Animation, AnimationExt, FocusHandle, FontWeight, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, SharedString, Styled, div, prelude::*, pulsating_between, px, rgb, svg,
};

use crate::actions::CommandAction;
use crate::theme::{
    AURORA_ACCENT_STRONG, AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG, AURORA_FONT_DISPLAY,
    AURORA_FONT_MONO, AURORA_HOVER_BG, AURORA_NAV_BG, AURORA_TEXT_MUTED, AURORA_TEXT_PRIMARY,
};
use crate::{Submit, ToggleActionMenu};

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
        .child(render_brand_button(action_menu_open))
        .when_some(locked_command, |el, action| {
            el.child(render_locked_badge(action))
        })
        .child(render_prompt_text(prompt, query_is_empty, input_active))
        .child(render_send_button())
}

fn render_brand_button(action_menu_open: bool) -> impl IntoElement {
    div()
        .cursor_pointer()
        .rounded_sm()
        .px_1()
        .py_1()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
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
        .child(svg().path("axon-glyph.svg").w(px(17.0)).h(px(17.0)))
        .child("axon ▾")
}

fn render_locked_badge(action: CommandAction) -> impl IntoElement {
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
        .child(action.subcommand)
}

fn render_prompt_text(
    prompt: SharedString,
    query_is_empty: bool,
    input_active: bool,
) -> impl IntoElement {
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
        .when(input_active, |el| el.child(render_caret()))
}

fn render_caret() -> impl IntoElement {
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
        )
}

fn render_send_button() -> impl IntoElement {
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
        .child("send")
}
