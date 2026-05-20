//! Main palette body renderer, split out to keep `ui_render.rs` under the
//! project monolith cap.

use gpui::{
    AnyElement, Context, IntoElement, MouseDownEvent, ParentElement, ScrollHandle, SharedString,
    Styled, Window, div, prelude::*, px, rgb,
};

use super::Palette;
use crate::actions::CommandAction;
use crate::output::CommandOutput;
use crate::render::{
    render_action_rows_interactive, render_output_body, render_palette_footer, render_prompt_row,
};
use crate::theme::{
    AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG, AURORA_NAV_BG, AURORA_PANEL_STRONG,
};

pub(super) fn render_palette_body(
    palette: &mut Palette,
    actions: Vec<CommandAction>,
    selected: usize,
    running_subcommand: Option<&'static str>,
    hide_list: bool,
    selected_action: Option<CommandAction>,
    command_output: Option<CommandOutput>,
    locked: Option<CommandAction>,
    prompt: SharedString,
    query_is_empty: bool,
    action_menu_open: bool,
    status_dot: AnyElement,
    window: &mut Window,
    cx: &mut Context<Palette>,
) -> impl IntoElement {
    div()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .p_5()
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .mx_auto()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .rounded_md()
                .bg(rgb(AURORA_PANEL_STRONG))
                .border_1()
                .border_color(rgb(AURORA_BORDER_STRONG))
                .shadow_lg()
                .child(render_prompt_row(
                    query_is_empty,
                    locked,
                    prompt,
                    palette.focus.is_focused(window),
                    palette.focus.clone(),
                    action_menu_open,
                    status_dot,
                ))
                .child(render_action_rows_interactive(
                    actions,
                    selected,
                    running_subcommand,
                    hide_list,
                    &palette.action_scroll,
                    |i| {
                        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                            this.selected = i;
                            if this.action_menu_open {
                                if let Some(action) = this.matches().get(i).copied() {
                                    this.select_action_mode(action, false, cx);
                                }
                            } else {
                                this.submit(&crate::Submit, window, cx);
                            }
                        })
                    },
                    |i| {
                        cx.listener(move |this, hovered: &bool, _window, cx| {
                            if *hovered && this.selected != i {
                                this.selected = i;
                                cx.notify();
                            }
                        })
                    },
                ))
                .when_some(selected_action, |el, action| {
                    el.child(render_palette_footer(
                        action,
                        command_output.as_ref(),
                        palette.running.as_ref(),
                        palette.conversation_hint(),
                    ))
                })
                .when_some(
                    command_output.clone().filter(|o| o.has_body()),
                    |el, output| {
                        el.child(render_output_panel(
                            output,
                            palette.errors_open,
                            &palette.output_scroll,
                        ))
                    },
                ),
        )
}

fn render_output_panel(
    output: CommandOutput,
    errors_open: bool,
    output_scroll: &ScrollHandle,
) -> impl IntoElement {
    div()
        .id("palette-output")
        .flex_1()
        .overflow_scroll()
        .scrollbar_width(px(12.0))
        .track_scroll(output_scroll)
        .block_mouse_except_scroll()
        .border_t_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .bg(rgb(AURORA_NAV_BG))
        .child(render_output_body(output, errors_open))
}
