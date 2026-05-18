use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, App, Div, FontWeight, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, SharedString, Stateful, Styled, Window, div, ease_in_out,
    prelude::*, px, rgb,
};

use crate::actions::{ArgMode, CommandAction};
use crate::theme::{
    AURORA_ACCENT_PRIMARY, AURORA_ACCENT_STRONG, AURORA_BORDER_DEFAULT, AURORA_FONT_MONO,
    AURORA_HOVER_BG, AURORA_NAV_BG, AURORA_PANEL_STRONG, AURORA_PRESSED_BG, AURORA_ROW_HOVER_BG,
    AURORA_TEXT_MUTED, AURORA_TEXT_PRIMARY,
};

pub(crate) fn render_action_rows_interactive<F, L, H, R>(
    actions: Vec<CommandAction>,
    selected: usize,
    running_subcommand: Option<&'static str>,
    hide_list: bool,
    mut make_on_click: F,
    mut make_on_hover: H,
) -> impl IntoElement
where
    F: FnMut(usize) -> L,
    L: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    H: FnMut(usize) -> R,
    R: Fn(&bool, &mut Window, &mut App) + 'static,
{
    let is_empty = actions.is_empty();
    let rows: Vec<AnyElement> = actions
        .into_iter()
        .enumerate()
        .map(|(i, action)| {
            render_action_row(
                action,
                i,
                i == selected,
                running_subcommand == Some(action.subcommand),
            )
            .on_mouse_down(MouseButton::Left, make_on_click(i))
            .on_hover(make_on_hover(i))
            .with_animation(
                ("axon-action-row-fade", i),
                Animation::new(Duration::from_millis(140)).with_easing(ease_in_out),
                |el, t| el.opacity(t),
            )
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
    let hover_bg = if is_selected {
        AURORA_HOVER_BG
    } else {
        AURORA_ROW_HOVER_BG
    };
    let accent_bar = div()
        .w(px(3.0))
        .h(px(20.0))
        .rounded_full()
        .flex_shrink_0()
        .bg(rgb(if is_selected {
            AURORA_ACCENT_PRIMARY
        } else {
            0x00000000
        }));

    div()
        .id(("axon-action-row", index))
        .h(px(38.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .mx_1()
        .pl_2()
        .pr_3()
        .rounded_md()
        .cursor_pointer()
        .border_1()
        .border_color(rgb(if is_selected {
            AURORA_ACCENT_PRIMARY
        } else {
            0x00000000
        }))
        .bg(rgb(base_bg))
        .hover(move |el| el.bg(rgb(hover_bg)))
        .active(|el| el.bg(rgb(AURORA_PRESSED_BG)))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(accent_bar)
                .child(
                    div()
                        .font_weight(if is_selected {
                            FontWeight(640.0)
                        } else {
                            FontWeight(500.0)
                        })
                        .text_size(px(13.0))
                        .text_color(if is_selected {
                            rgb(AURORA_TEXT_PRIMARY)
                        } else {
                            rgb(AURORA_TEXT_MUTED)
                        })
                        .child(SharedString::from(action.label)),
                ),
        )
        .child(
            div()
                .px_2()
                .py(px(2.0))
                .rounded_sm()
                .border_1()
                .border_color(rgb(AURORA_BORDER_DEFAULT))
                .bg(rgb(AURORA_NAV_BG))
                .font_family(AURORA_FONT_MONO)
                .font_weight(FontWeight(580.0))
                .text_size(px(10.0))
                .text_color(rgb(meta_color))
                .child(meta),
        )
}
