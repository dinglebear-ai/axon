//! `Render` impl for `Palette`, split out from `ui.rs` to keep that file
//! under the project monolith cap. Declared via `#[path]` in `ui.rs` so it
//! remains a child of `ui`, retaining access to `Palette`'s private items.

use gpui::{
    AnyElement, Context, IntoElement, MouseButton, MouseDownEvent, ParentElement, Render,
    ScrollHandle, SharedString, Styled, Window, div, prelude::*, px, rgb,
};

use super::Palette;
use crate::layout::compute_desired_height;
use crate::output::CommandOutput;
use crate::render::{
    render_action_rows_interactive, render_output_body, render_palette_footer, render_prompt_row,
};
use crate::theme::{
    AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG, AURORA_FONT_SANS, AURORA_NAV_BG, AURORA_PAGE_BG,
    AURORA_PANEL_STRONG, AURORA_TEXT_PRIMARY,
};

impl Render for Palette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let actions = self.matches();
        let selected = self.selected;
        let selected_action = actions.get(selected).copied();
        let running_subcommand = self.running.as_ref().map(|running| running.subcommand);
        let command_output = self.command_output.clone();
        let locked = self.locked_command;
        let action_menu_open = self.action_menu_open;
        let hide_list = !action_menu_open && (self.query.is_empty() || locked.is_some());

        // Drive the content-driven window resize. This runs every render
        // pass but `sync_window_height` is idempotent — it only calls
        // `Window::resize` when the computed target differs from the last
        // committed value.
        let target_height = compute_desired_height(self.height_snapshot(&actions, hide_list));
        self.sync_window_height(target_height, window);

        let prompt: SharedString = if self.query.is_empty() {
            if let Some(action) = locked {
                let hint = action
                    .example
                    .splitn(2, ' ')
                    .nth(1)
                    .unwrap_or(action.example);
                SharedString::from(hint.to_string())
            } else {
                SharedString::from("type a command or URL")
            }
        } else {
            SharedString::from(format!("> {}", self.query))
        };
        let query_is_empty = self.query.is_empty();

        let status_dot = render_status_dot(self.connection, cx);

        div()
            .key_context("Palette")
            .track_focus(&self.focus)
            .on_action(cx.listener(Palette::submit))
            .on_action(cx.listener(Palette::move_down))
            .on_action(cx.listener(Palette::move_up))
            .on_action(cx.listener(Palette::tab_complete))
            .on_action(cx.listener(Palette::clear_output))
            .on_action(cx.listener(Palette::toggle_action_menu))
            .on_action(cx.listener(Palette::toggle_errors))
            .on_key_down(cx.listener(Palette::on_key))
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .font_family(AURORA_FONT_SANS)
            .bg(rgb(AURORA_PAGE_BG))
            .text_color(rgb(AURORA_TEXT_PRIMARY))
            .p_5()
            .child(
                div()
                    .w_full()
                    .max_w(px(760.0))
                    .mx_auto()
                    .flex()
                    .flex_col()
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
                        self.focus.is_focused(window),
                        self.focus.clone(),
                        action_menu_open,
                        status_dot,
                    ))
                    .child(render_action_rows_interactive(
                        actions,
                        selected,
                        running_subcommand,
                        hide_list,
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
                            self.running.as_ref(),
                            self.conversation_hint(),
                        ))
                    })
                    .when_some(
                        command_output.clone().filter(|o| o.has_body()),
                        |el, output| {
                            el.child(render_output_panel(
                                output,
                                self.errors_open,
                                &self.output_scroll,
                            ))
                        },
                    ),
            )
    }
}

fn render_status_dot(connection: super::ConnectionState, cx: &mut Context<Palette>) -> AnyElement {
    // Intentionally non-animated. A launch-time pulsing health dot previously
    // kept slower compositors repainting every frame and could starve key input.
    div()
        .id("status-dot")
        .size(px(8.0))
        .rounded_full()
        .flex_shrink_0()
        .cursor_pointer()
        .bg(rgb(connection.dot_color()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _: &MouseDownEvent, _window, cx| {
                this.spawn_health_check(cx);
            }),
        )
        .into_any_element()
}

fn render_output_panel(
    output: CommandOutput,
    errors_open: bool,
    output_scroll: &ScrollHandle,
) -> impl IntoElement {
    div()
        .id("palette-output")
        .max_h(px(320.0))
        .overflow_scroll()
        .scrollbar_width(px(12.0))
        .track_scroll(output_scroll)
        .block_mouse_except_scroll()
        .border_t_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .bg(rgb(AURORA_NAV_BG))
        .child(render_output_body(output, errors_open))
}
