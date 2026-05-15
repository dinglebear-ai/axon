//! axon-palette: a global-hotkey command palette for the axon CLI.
//!
//! Press the configured global hotkey (default: Ctrl+Shift+Space) anywhere on
//! the desktop to bring the palette window forward. Type to filter actions,
//! optionally followed by an argument (URL, query, etc.), then press Enter.
//! The palette shells out to the `axon` binary on $PATH.

use std::process::Command;
use std::thread;

use anyhow::Result;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use gpui::{
    App, Application, Bounds, Context, FocusHandle, Focusable, IntoElement, KeyBinding,
    ParentElement, Render, SharedString, Styled, TitlebarOptions, Window, WindowBounds,
    WindowOptions, actions, div, prelude::*, px, rgb, size,
};

#[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "windows")))]
compile_error!("axon-palette currently supports Linux/FreeBSD/Windows only");

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn build_application() -> Application {
    Application::with_platform(gpui_linux::current_platform(false))
}

#[cfg(target_os = "windows")]
fn build_application() -> Application {
    Application::with_platform(std::rc::Rc::new(
        gpui_windows::WindowsPlatform::new(false)
            .expect("failed to initialize Windows platform"),
    ))
}

#[derive(Clone, Copy)]
struct CommandAction {
    label: &'static str,
    /// `axon` subcommand to invoke. The optional user argument is passed as a
    /// single trailing argv entry when `needs_arg` is true.
    subcommand: &'static str,
    needs_arg: bool,
}

const ACTIONS: &[CommandAction] = &[
    CommandAction { label: "Scrape URL",       subcommand: "scrape",   needs_arg: true  },
    CommandAction { label: "Crawl URL",        subcommand: "crawl",    needs_arg: true  },
    CommandAction { label: "Map URL",          subcommand: "map",      needs_arg: true  },
    CommandAction { label: "Ask question",     subcommand: "ask",      needs_arg: true  },
    CommandAction { label: "Search the web",   subcommand: "search",   needs_arg: true  },
    CommandAction { label: "Research the web", subcommand: "research", needs_arg: true  },
    CommandAction { label: "Ingest target",    subcommand: "ingest",   needs_arg: true  },
    CommandAction { label: "Job status",       subcommand: "status",   needs_arg: false },
    CommandAction { label: "Doctor",           subcommand: "doctor",   needs_arg: false },
];

actions!(palette, [Submit, MoveDown, MoveUp]);

struct Palette {
    query: String,
    selected: usize,
    focus: FocusHandle,
    last_status: Option<SharedString>,
}

impl Palette {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            query: String::new(),
            selected: 0,
            focus: cx.focus_handle(),
            last_status: None,
        }
    }

    fn matches(&self) -> Vec<CommandAction> {
        let head = self
            .query
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();
        ACTIONS
            .iter()
            .copied()
            .filter(|a| head.is_empty() || a.label.to_lowercase().contains(&head))
            .collect()
    }

    fn submit(&mut self, _: &Submit, _window: &mut Window, cx: &mut Context<Self>) {
        let actions = self.matches();
        let Some(action) = actions.get(self.selected).copied() else {
            return;
        };

        let arg = self
            .query
            .splitn(2, ' ')
            .nth(1)
            .map(str::trim)
            .unwrap_or("");

        if action.needs_arg && arg.is_empty() {
            self.last_status = Some("argument required".into());
            cx.notify();
            return;
        }

        // `arg` is passed as a single argv entry so multi-word inputs (e.g.
        // `ask what is embedding`) survive intact.
        let sub = action.subcommand;

        let mut cmd = Command::new("axon");
        cmd.arg(sub);
        if action.needs_arg {
            cmd.arg(arg);
        }

        match cmd.spawn() {
            Ok(mut child) => {
                self.last_status =
                    Some(format!("spawned axon {sub} (pid {})", child.id()).into());
                // Reap the child off-thread so it doesn't become a zombie on
                // Unix. The palette is a long-lived UI process, so dropping
                // Child without waiting would accumulate <defunct> entries.
                thread::spawn(move || {
                    let _ = child.wait();
                });
                self.query.clear();
                self.selected = 0;
            }
            Err(e) => {
                self.last_status = Some(format!("failed to spawn axon: {e}").into());
            }
        }
        cx.notify();
    }

    fn move_down(&mut self, _: &MoveDown, _w: &mut Window, cx: &mut Context<Self>) {
        let n = self.matches().len();
        if n > 0 {
            self.selected = (self.selected + 1).min(n - 1);
            cx.notify();
        }
    }

    fn move_up(&mut self, _: &MoveUp, _w: &mut Window, cx: &mut Context<Self>) {
        self.selected = self.selected.saturating_sub(1);
        cx.notify();
    }

    fn on_key(
        &mut self,
        ev: &gpui::KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = ev.keystroke.key.as_str();
        match key {
            "backspace" => {
                self.query.pop();
            }
            "escape" => {
                self.query.clear();
            }
            _ => {
                // Only accept plain text input: ignore Ctrl/Alt/Cmd/Function
                // combos so an unrelated shortcut (e.g. Alt+letter producing
                // an accented char on some layouts) doesn't leak into the
                // query buffer. Shift is intentionally allowed so the user
                // can type uppercase letters.
                let m = &ev.keystroke.modifiers;
                if !m.control && !m.alt && !m.platform && !m.function {
                    if let Some(ch) = ev.keystroke.key_char.as_deref() {
                        self.query.push_str(ch);
                    }
                }
            }
        }
        self.selected = 0;
        cx.notify();
    }
}

impl Focusable for Palette {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Palette {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let actions = self.matches();
        let selected = self.selected;
        let prompt = if self.query.is_empty() {
            SharedString::from("type a command…")
        } else {
            SharedString::from(format!("> {}", self.query))
        };

        div()
            .key_context("Palette")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::move_up))
            .on_key_down(cx.listener(Self::on_key))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xeeeeee))
            .p_4()
            .child(
                div()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(0x313244))
                    .child(prompt),
            )
            .child(
                div()
                    .mt_3()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(actions.into_iter().enumerate().map(move |(i, action)| {
                        let is_sel = i == selected;
                        div()
                            .px_3()
                            .py_1()
                            .rounded_sm()
                            .bg(if is_sel { rgb(0x45475a) } else { rgb(0x00000000) })
                            .child(SharedString::from(action.label))
                    })),
            )
            .when_some(self.last_status.clone(), |el, status| {
                el.child(div().mt_4().text_color(rgb(0xa6e3a1)).child(status))
            })
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Register the global hotkey BEFORE the GPUI event loop so we fail fast if
    // the WM/compositor refuses the binding.
    let manager = GlobalHotKeyManager::new()?;
    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
    manager.register(hotkey)?;
    let hotkey_id = hotkey.id();
    tracing::info!("registered global hotkey: Ctrl+Shift+Space (id={hotkey_id})");

    let (tx, rx) = async_channel::unbounded::<()>();
    thread::spawn(move || {
        let receiver = GlobalHotKeyEvent::receiver();
        while let Ok(ev) = receiver.recv() {
            if ev.id == hotkey_id && ev.state == HotKeyState::Pressed {
                let _ = tx.send_blocking(());
            }
        }
    });

    build_application().run(move |cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("enter", Submit, Some("Palette")),
            KeyBinding::new("down", MoveDown, Some("Palette")),
            KeyBinding::new("up", MoveUp, Some("Palette")),
        ]);

        let bounds = Bounds::centered(None, size(px(640.0), px(420.0)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("axon palette".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(Palette::new);
                    let handle = view.focus_handle(cx);
                    window.focus(&handle, cx);
                    view
                },
            )
            .expect("failed to open window");

        // Bridge global hotkey events into the GPUI main thread.
        cx.spawn(async move |cx| {
            while rx.recv().await.is_ok() {
                let _ = window.update(cx, |_root, window, _cx| {
                    window.activate_window();
                });
            }
        })
        .detach();

        // Keep the GlobalHotKeyManager alive for the lifetime of the app.
        // Without this, dropping the manager would unregister the hotkey.
        std::mem::forget(manager);
    });

    Ok(())
}
