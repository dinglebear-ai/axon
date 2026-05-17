#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

//! axon-palette: a global-hotkey command palette for the axon CLI.
//!
//! Press the configured global hotkey (default: Ctrl+Shift+Space) anywhere on
//! the desktop to bring the palette window forward. Type to filter actions,
//! optionally followed by an argument (URL, query, etc.), then press Enter.
//! The palette shells out to the `axon` binary on $PATH.

mod actions;
mod anim;
mod layout;
mod markdown;
mod output;
mod render;
mod theme;
mod ui;

use std::thread;

use anyhow::Result;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use gpui::{
    App, Application, Bounds, Focusable, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions,
    actions, prelude::*, px, size,
};

use crate::theme::register_bundled_fonts;
use crate::ui::Palette;

#[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "windows")))]
compile_error!("axon-palette currently supports Linux/FreeBSD/Windows only");

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn build_application() -> Application {
    Application::with_platform(gpui_linux::current_platform(false))
}

#[cfg(target_os = "windows")]
fn build_application() -> Application {
    Application::with_platform(std::rc::Rc::new(
        gpui_windows::WindowsPlatform::new(false).expect("failed to initialize Windows platform"),
    ))
}

actions!(
    palette,
    [Submit, MoveDown, MoveUp, TabComplete, ClearOutput]
);

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
        register_bundled_fonts(cx);

        cx.bind_keys([
            KeyBinding::new("enter", Submit, Some("Palette")),
            KeyBinding::new("down", MoveDown, Some("Palette")),
            KeyBinding::new("up", MoveUp, Some("Palette")),
            KeyBinding::new("tab", TabComplete, Some("Palette")),
        ]);

        // Launch height is the prompt-only minimum from `layout::MIN_WINDOW_HEIGHT`.
        // The window then grows on demand as the user types, runs commands,
        // and produces output. See `Palette::sync_window_height`.
        let bounds = Bounds::centered(
            None,
            size(px(720.0), px(crate::layout::MIN_WINDOW_HEIGHT)),
            cx,
        );
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: None,
                        appears_transparent: true,
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
