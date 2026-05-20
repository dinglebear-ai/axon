//! `Render` impl for `Palette`, split out from `ui.rs` to keep that file
//! under the project monolith cap. Declared via `#[path]` in `ui.rs` so it
//! remains a child of `ui`, retaining access to `Palette`'s private items.

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Render, SharedString, Styled, Window, WindowControlArea, div, prelude::*, px, rgb,
};

use super::ui_body::render_palette_body;
use super::{ChromeMenu, Palette};
use crate::layout::compute_desired_height;
use crate::theme::{
    AURORA_ACCENT_PRIMARY, AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG, AURORA_FONT_MONO,
    AURORA_FONT_SANS, AURORA_PAGE_BG, AURORA_TEXT_MUTED, AURORA_TEXT_PRIMARY,
};
use crate::{Submit, ToggleActionMenu};

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

        let prompt = render_prompt_text(self, locked);
        let query_is_empty = self.query.is_empty();

        let status_dot = render_status_dot(self.connection, cx);

        let chrome_menu_open = self.chrome_menu_open;
        let has_output = command_output
            .as_ref()
            .is_some_and(|output| output.has_body());

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
            .relative()
            .overflow_hidden()
            .font_family(AURORA_FONT_SANS)
            .bg(rgb(AURORA_PAGE_BG))
            .text_color(rgb(AURORA_TEXT_PRIMARY))
            .child(render_window_chrome(
                action_menu_open,
                chrome_menu_open,
                window,
                cx,
            ))
            .child(render_palette_body(
                self,
                actions,
                selected,
                running_subcommand,
                hide_list,
                selected_action,
                command_output,
                locked,
                prompt,
                query_is_empty,
                action_menu_open,
                status_dot,
                window,
                cx,
            ))
            .when_some(chrome_menu_open, |el, menu| {
                el.child(render_chrome_menu_dropdown(menu, has_output, cx))
            })
    }
}

fn render_prompt_text(
    palette: &Palette,
    locked: Option<crate::actions::CommandAction>,
) -> SharedString {
    if !palette.query.is_empty() {
        return SharedString::from(format!("> {}", palette.query));
    }

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
}

fn render_window_chrome(
    action_menu_open: bool,
    chrome_menu_open: Option<ChromeMenu>,
    window: &mut Window,
    cx: &mut Context<Palette>,
) -> impl IntoElement {
    div()
        .id("axon-window-chrome")
        .h(px(crate::layout::WINDOW_CHROME_HEIGHT))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .window_control_area(WindowControlArea::Drag)
        .border_b_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .bg(rgb(0x111a24))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .px_2()
                .h_full()
                .child(render_chrome_menu_trigger(action_menu_open))
                .child(render_menu_item(
                    "File",
                    ChromeMenu::File,
                    chrome_menu_open,
                    cx,
                ))
                .child(render_menu_item(
                    "Edit",
                    ChromeMenu::Edit,
                    chrome_menu_open,
                    cx,
                ))
                .child(render_menu_item(
                    "View",
                    ChromeMenu::View,
                    chrome_menu_open,
                    cx,
                ))
                .child(render_menu_item(
                    "Run",
                    ChromeMenu::Run,
                    chrome_menu_open,
                    cx,
                ))
                .child(render_menu_item(
                    "Help",
                    ChromeMenu::Help,
                    chrome_menu_open,
                    cx,
                )),
        )
        .child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .overflow_hidden()
                .child(
                    div()
                        .font_family(AURORA_FONT_SANS)
                        .font_weight(FontWeight(560.0))
                        .text_size(px(12.0))
                        .text_color(rgb(AURORA_TEXT_MUTED))
                        .child("axon"),
                ),
        )
        .child(render_windows_caption_buttons(window))
}

fn render_chrome_menu_trigger(action_menu_open: bool) -> impl IntoElement {
    div()
        .id("chrome-menu-trigger")
        .occlude()
        .h(px(24.0))
        .px_2()
        .flex()
        .items_center()
        .rounded_sm()
        .text_size(px(13.0))
        .text_color(if action_menu_open {
            rgb(AURORA_ACCENT_PRIMARY)
        } else {
            rgb(AURORA_TEXT_MUTED)
        })
        .hover(|el| el.bg(rgb(0x1a2633)).text_color(rgb(AURORA_TEXT_PRIMARY)))
        .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, window, cx| {
            window.dispatch_action(Box::new(ToggleActionMenu), cx);
        })
        .child("≡")
}

fn render_menu_item(
    label: &'static str,
    menu: ChromeMenu,
    chrome_menu_open: Option<ChromeMenu>,
    cx: &mut Context<Palette>,
) -> impl IntoElement {
    let is_open = chrome_menu_open == Some(menu);
    div()
        .id(SharedString::from(format!("chrome-menu-{label}")))
        .occlude()
        .h(px(24.0))
        .px_2()
        .flex()
        .items_center()
        .rounded_sm()
        .font_family(AURORA_FONT_SANS)
        .font_weight(FontWeight(520.0))
        .text_size(px(12.0))
        .text_color(if is_open {
            rgb(AURORA_TEXT_PRIMARY)
        } else {
            rgb(AURORA_TEXT_MUTED)
        })
        .when(is_open, |el| el.bg(rgb(0x1a2633)))
        .hover(|el| el.bg(rgb(0x1a2633)).text_color(rgb(AURORA_TEXT_PRIMARY)))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, _window, cx| {
                this.toggle_chrome_menu(menu, cx);
            }),
        )
        .child(label)
}

#[derive(Clone, Copy)]
enum MenuCommand {
    ClearOutput,
    ClearInput,
    Submit,
    ShowCommands,
    ToggleErrors,
}

fn render_chrome_menu_dropdown(
    menu: ChromeMenu,
    has_output: bool,
    cx: &mut Context<Palette>,
) -> impl IntoElement {
    let left = match menu {
        ChromeMenu::File => 38.0,
        ChromeMenu::Edit => 78.0,
        ChromeMenu::View => 119.0,
        ChromeMenu::Run => 166.0,
        ChromeMenu::Help => 208.0,
    };

    let rows: Vec<(&'static str, MenuCommand, bool)> = match menu {
        ChromeMenu::File => vec![
            ("Clear Output", MenuCommand::ClearOutput, has_output),
            ("Show Commands", MenuCommand::ShowCommands, true),
        ],
        ChromeMenu::Edit => vec![
            ("Clear Input", MenuCommand::ClearInput, true),
            ("Show Commands", MenuCommand::ShowCommands, true),
        ],
        ChromeMenu::View => vec![
            ("Toggle Errors", MenuCommand::ToggleErrors, has_output),
            ("Clear Output", MenuCommand::ClearOutput, has_output),
        ],
        ChromeMenu::Run => vec![("Run Command", MenuCommand::Submit, true)],
        ChromeMenu::Help => vec![("Show Commands", MenuCommand::ShowCommands, true)],
    };

    let row_elements: Vec<AnyElement> = rows
        .into_iter()
        .map(|(label, command, enabled)| {
            render_chrome_menu_row(label, command, enabled, cx).into_any_element()
        })
        .collect();

    div()
        .id("chrome-menu-dropdown")
        .absolute()
        .top(px(crate::layout::WINDOW_CHROME_HEIGHT - 1.0))
        .left(px(left))
        .w(px(180.0))
        .py_1()
        .rounded_sm()
        .border_1()
        .border_color(rgb(AURORA_BORDER_STRONG))
        .bg(rgb(0x111a24))
        .shadow_lg()
        .children(row_elements)
}

fn render_chrome_menu_row(
    label: &'static str,
    command: MenuCommand,
    enabled: bool,
    cx: &mut Context<Palette>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("chrome-menu-row-{label}")))
        .occlude()
        .h(px(28.0))
        .px_3()
        .flex()
        .items_center()
        .font_family(AURORA_FONT_SANS)
        .font_weight(FontWeight(520.0))
        .text_size(px(12.0))
        .text_color(if enabled {
            rgb(AURORA_TEXT_PRIMARY)
        } else {
            rgb(AURORA_TEXT_MUTED)
        })
        .when(enabled, |el| el.hover(|el| el.bg(rgb(0x1a2633))))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                if !enabled {
                    return;
                }
                this.chrome_menu_open = None;
                match command {
                    MenuCommand::ClearOutput => {
                        this.command_output = None;
                        this.errors_open = false;
                        cx.notify();
                    }
                    MenuCommand::ClearInput => {
                        this.query.clear();
                        this.locked_command = None;
                        this.selected = 0;
                        cx.notify();
                    }
                    MenuCommand::Submit => this.submit(&Submit, window, cx),
                    MenuCommand::ShowCommands => {
                        this.action_menu_open = true;
                        this.selected = this.selected_for_locked_action();
                        cx.notify();
                    }
                    MenuCommand::ToggleErrors => {
                        this.errors_open = !this.errors_open;
                        cx.notify();
                    }
                }
            }),
        )
        .child(label)
}

fn render_windows_caption_buttons(window: &mut Window) -> impl IntoElement {
    div()
        .id("windows-window-controls")
        .font_family(windows_caption_font())
        .flex()
        .flex_row()
        .h_full()
        .child(render_caption_button(
            "minimize",
            "\u{e921}",
            WindowControlArea::Min,
        ))
        .child(if window.is_maximized() {
            render_caption_button("restore", "\u{e923}", WindowControlArea::Max)
        } else {
            render_caption_button("maximize", "\u{e922}", WindowControlArea::Max)
        })
        .child(render_close_button())
}

fn render_caption_button(
    id: &'static str,
    icon: &'static str,
    control: WindowControlArea,
) -> impl IntoElement {
    div()
        .id(id)
        .occlude()
        .w(px(36.0))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(10.0))
        .text_color(rgb(AURORA_TEXT_PRIMARY))
        .window_control_area(control)
        .hover(|el| el.bg(rgb(0x243242)))
        .active(|el| el.bg(rgb(0x2c3d4f)))
        .child(icon)
}

fn render_close_button() -> impl IntoElement {
    div()
        .id("close")
        .occlude()
        .w(px(36.0))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(10.0))
        .text_color(rgb(AURORA_TEXT_PRIMARY))
        .window_control_area(WindowControlArea::Close)
        .hover(|el| el.bg(rgb(0xe81120)).text_color(rgb(0xffffff)))
        .active(|el| el.bg(rgb(0xb50d18)).text_color(rgb(0xffffff)))
        .child("\u{e8bb}")
}

#[cfg(target_os = "windows")]
fn windows_caption_font() -> &'static str {
    "Segoe Fluent Icons"
}

#[cfg(not(target_os = "windows"))]
fn windows_caption_font() -> &'static str {
    AURORA_FONT_MONO
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
