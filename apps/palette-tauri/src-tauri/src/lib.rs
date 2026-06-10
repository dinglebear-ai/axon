use std::{
    collections::HashMap,
    fmt::Display,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Size, WindowEvent,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

mod axon_bridge;
mod persistence;
mod stream;

use axon_bridge::{BridgeClient, StreamClient};
use persistence::*;
use stream::axon_http_stream_request;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PaletteSettings {
    server_url: String,
    token: Option<String>,
    shortcut: String,
    collection: String,
    result_limit: u16,
    theme: PaletteTheme,
    hide_on_blur: bool,
    open_results_inline: bool,
    env_values: HashMap<String, serde_json::Value>,
    config_values: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum PaletteTheme {
    System,
    Dark,
    Light,
}

const DEFAULT_SERVER_URL: &str = "https://axon.tootie.tv";
const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+Space";
const SETTINGS_FILE: &str = "settings.json";

// Runtime gate for hide-on-blur, toggled by the frontend. The launcher hides on
// blur (click-away dismiss), but while a result/settings view is open we keep it
// up so resizing or copying from another window doesn't make it vanish.
// Checked together with the `hide_on_blur` user preference in the
// `WindowEvent::Focused(false)` handler.
struct BlurDismiss(AtomicBool);

/// Tracks the shortcut label currently registered so we can unregister only
/// that specific shortcut (rather than calling `unregister_all`) when the user
/// changes the keybinding.
struct ActiveShortcut(Mutex<Option<String>>);

fn log_palette_warning(context: &str, err: impl Display) {
    eprintln!("axon palette: {context}: {err}");
}

#[tauri::command]
fn load_palette_config(app: AppHandle) -> Result<PaletteSettings, String> {
    merged_settings(&app)
}

#[tauri::command]
fn load_palette_default_config() -> PaletteSettings {
    default_settings(&read_default_env_entries())
}

#[tauri::command]
fn save_palette_settings(
    app: AppHandle,
    settings: PaletteSettings,
) -> Result<PaletteSettings, String> {
    let settings = normalize_settings(settings);
    // 1. Validate and persist Axon runtime config (env + toml) first so that
    //    file writes succeed before any in-process state is mutated.
    save_axon_env(&settings)?;
    save_axon_config(&settings)?;
    // 2. Persist palette-only preferences (no env/config keys).
    save_palette_prefs(&app, &settings)?;
    // 3. Only mutate runtime state (shortcut) after all writes succeed.
    update_shortcut(&app, &settings)?;
    Ok(settings_with_file_values(settings))
}

fn save_axon_env(settings: &PaletteSettings) -> Result<(), String> {
    write_axon_env_values(&settings.env_values).map_err(|err| err.to_string())
}

fn save_axon_config(settings: &PaletteSettings) -> Result<(), String> {
    write_axon_config_values(&settings.config_values).map_err(|err| err.to_string())
}

fn save_palette_prefs(app: &AppHandle, settings: &PaletteSettings) -> Result<(), String> {
    write_settings(app, settings).map_err(|err| err.to_string())
}

fn update_shortcut(app: &AppHandle, settings: &PaletteSettings) -> Result<(), String> {
    register_configured_shortcut(app, settings)
}

#[tauri::command]
fn hide_palette(app: AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?
        .hide()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn show_palette(app: AppHandle) -> Result<(), String> {
    show_main_window(&app)
}

#[tauri::command]
fn resize_palette(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    // A maximized window ignores set_size on Windows; drop maximize first so the
    // auto-sizer (and the next launcher open) always lands at the intended size.
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    }
    window
        .set_size(Size::Logical(LogicalSize { width, height }))
        .map_err(|err| err.to_string())?;
    window.center().map_err(|err| err.to_string())
}

#[tauri::command]
fn toggle_maximize(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    if window.is_maximized().map_err(|err| err.to_string())? {
        window.unmaximize().map_err(|err| err.to_string())
    } else {
        window.maximize().map_err(|err| err.to_string())
    }
}

#[tauri::command]
fn set_blur_dismiss(state: tauri::State<'_, BlurDismiss>, enabled: bool) {
    state.0.store(enabled, Ordering::Relaxed);
}

fn merged_settings(app: &AppHandle) -> Result<PaletteSettings, String> {
    let persisted = read_settings_result(app)?;
    let env_entries = read_default_env_entries();
    let defaults = default_settings(&env_entries);

    Ok(merge_settings(persisted, defaults))
}

fn merged_settings_or_default(app: &AppHandle) -> PaletteSettings {
    match merged_settings(app) {
        Ok(settings) => settings,
        Err(err) => {
            eprintln!("{err}");
            default_settings(&read_default_env_entries())
        }
    }
}

fn merge_settings(persisted: PartialPaletteSettings, defaults: PaletteSettings) -> PaletteSettings {
    normalize_settings(PaletteSettings {
        server_url: persisted
            .server_url
            .or(Some(defaults.server_url))
            .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string()),
        token: persisted.token.unwrap_or(defaults.token),
        shortcut: persisted
            .shortcut
            .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string()),
        collection: persisted.collection.unwrap_or(defaults.collection),
        result_limit: persisted.result_limit.unwrap_or(10),
        theme: persisted.theme.unwrap_or(PaletteTheme::System),
        hide_on_blur: persisted.hide_on_blur.unwrap_or(true),
        open_results_inline: persisted.open_results_inline.unwrap_or(true),
        env_values: defaults.env_values,
        config_values: defaults.config_values,
    })
}

fn default_settings(env_entries: &[(String, String)]) -> PaletteSettings {
    let server_url = value_for("AXON_SERVER_URL", env_entries)
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string());
    let token = value_for("AXON_MCP_HTTP_TOKEN", env_entries)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let collection = value_for("AXON_COLLECTION", env_entries)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "axon".to_string());

    PaletteSettings {
        server_url,
        token,
        shortcut: DEFAULT_SHORTCUT.to_string(),
        collection,
        result_limit: 10,
        theme: PaletteTheme::System,
        hide_on_blur: true,
        open_results_inline: true,
        env_values: env_entries
            .iter()
            .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
            .collect(),
        config_values: read_default_config_values(),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialPaletteSettings {
    server_url: Option<String>,
    token: Option<Option<String>>,
    shortcut: Option<String>,
    collection: Option<String>,
    result_limit: Option<u16>,
    theme: Option<PaletteTheme>,
    hide_on_blur: Option<bool>,
    open_results_inline: Option<bool>,
}

fn normalize_settings(mut settings: PaletteSettings) -> PaletteSettings {
    settings.server_url = normalize_server_url(&settings.server_url);
    if settings.server_url.is_empty() {
        settings.server_url = DEFAULT_SERVER_URL.to_string();
    }
    settings.token = settings
        .token
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty());
    settings.shortcut = normalize_shortcut_label(&settings.shortcut);
    settings.collection = settings.collection.trim().to_string();
    if settings.collection.is_empty() {
        settings.collection = "axon".to_string();
    }
    settings.result_limit = settings.result_limit.clamp(1, 50);
    settings
}

fn normalize_server_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() || trimmed.contains("://") {
        trimmed.to_string()
    } else if trimmed.starts_with("localhost") || trimmed.starts_with("127.0.0.1") {
        format!("http://{trimmed}")
    } else {
        format!("https://{trimmed}")
    }
}

fn normalize_shortcut_label(shortcut: &str) -> String {
    match shortcut.trim().to_ascii_lowercase().as_str() {
        "alt+space" | "option+space" => "Alt+Space".to_string(),
        "ctrl+space" | "control+space" => "Ctrl+Space".to_string(),
        "cmd+shift+space" | "command+shift+space" | "super+shift+space" => {
            "Cmd+Shift+Space".to_string()
        }
        _ => DEFAULT_SHORTCUT.to_string(),
    }
}

/// Validate and normalise a saved Axon server URL.
///
/// Shared by `axon_bridge` and `stream` so they can't diverge silently.
pub(crate) fn validate_saved_server_url(server_url: &str) -> Result<String, String> {
    let server_url = normalize_server_url(server_url);
    let parsed = reqwest::Url::parse(&server_url)
        .map_err(|err| format!("saved Axon server URL is invalid: {err}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("saved Axon server URL must use http or https".to_string());
    }
    if parsed.host_str().is_none()
        || !matches!(parsed.path(), "" | "/")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("saved Axon server URL must be an origin URL".to_string());
    }
    Ok(server_url.trim_end_matches('/').to_string())
}

fn shortcut_for_label(label: &str) -> Shortcut {
    match normalize_shortcut_label(label).as_str() {
        "Alt+Space" => Shortcut::new(Some(Modifiers::ALT), Code::Space),
        "Ctrl+Space" => Shortcut::new(Some(Modifiers::CONTROL), Code::Space),
        "Cmd+Shift+Space" => Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space),
        _ => Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space),
    }
}

fn register_configured_shortcut(app: &AppHandle, settings: &PaletteSettings) -> Result<(), String> {
    let new_label = normalize_shortcut_label(&settings.shortcut);
    let new_shortcut = shortcut_for_label(&new_label);

    // Unregister only the previously registered shortcut if we know what it is,
    // rather than calling `unregister_all` which would also unregister shortcuts
    // registered by other parts of the app.
    if let Ok(mut guard) = app.state::<ActiveShortcut>().0.lock() {
        if let Some(old_label) = guard.take().filter(|l| l != &new_label) {
            let old_shortcut = shortcut_for_label(&old_label);
            if let Err(err) = app.global_shortcut().unregister(old_shortcut) {
                eprintln!("palette: failed to unregister old shortcut '{old_label}': {err}");
            }
        }
        app.global_shortcut()
            .register(new_shortcut)
            .map_err(|err| err.to_string())?;
        *guard = Some(new_label);
    } else {
        // Mutex poisoned — fall back to unregister_all for safety.
        app.global_shortcut()
            .unregister_all()
            .map_err(|err| err.to_string())?;
        app.global_shortcut()
            .register(new_shortcut)
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn show_main_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    }
    window
        .set_size(Size::Logical(LogicalSize {
            width: 680.0,
            height: 56.0,
        }))
        .map_err(|err| err.to_string())?;
    window.center().map_err(|err| err.to_string())?;
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    if let Err(err) = window.emit("palette://shown", ()) {
        log_palette_warning("failed to emit shown event", err);
    }
    Ok(())
}

fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    match window.is_visible() {
        Ok(true) => {
            if let Err(err) = window.hide() {
                log_palette_warning("failed to hide main window", err);
            }
        }
        _ => {
            if let Err(err) = show_main_window(app) {
                log_palette_warning("failed to show main window", err);
            }
        }
    }
}

fn install_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Palette", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Axon Palette", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &settings, &quit])?;

    let icon = app.default_window_icon().cloned();
    let mut tray = TrayIconBuilder::new()
        .tooltip("Axon Palette")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Err(err) = show_main_window(app) {
                    log_palette_warning("failed to show main window from tray", err);
                }
            }
            "settings" => {
                if let Err(err) = show_main_window(app) {
                    log_palette_warning("failed to show main window for settings", err);
                }
                if let Some(window) = app.get_webview_window("main") {
                    if let Err(err) = window.emit("palette://open-settings", ()) {
                        log_palette_warning("failed to emit open settings event", err);
                    }
                } else {
                    log_palette_warning("failed to open settings", "main window not found");
                }
            }
            "quit" => {
                if let Err(err) = app.global_shortcut().unregister_all() {
                    log_palette_warning("failed to unregister global shortcuts on quit", err);
                }
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = icon {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let bridge_client = BridgeClient::new()
        .map_err(|err| format!("failed to build HTTP client for Axon bridge: {err}"))?;
    let stream_client = StreamClient::new()
        .map_err(|err| format!("failed to build HTTP client for streaming: {err}"))?;

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_main_window(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            load_palette_config,
            load_palette_default_config,
            save_palette_settings,
            hide_palette,
            show_palette,
            resize_palette,
            toggle_maximize,
            set_blur_dismiss,
            axon_bridge::axon_http_request,
            axon_http_stream_request
        ])
        .manage(BlurDismiss(AtomicBool::new(true)))
        .manage(ActiveShortcut(Mutex::new(None)))
        .manage(bridge_client)
        .manage(stream_client)
        .setup(|app| {
            if let Err(err) = install_tray(app) {
                log_palette_warning("failed to install tray icon", err);
            }
            let settings = merged_settings_or_default(app.handle());
            register_configured_shortcut(app.handle(), &settings).map_err(anyhow::Error::msg)?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let window_handle = handle.clone();
                if let Err(err) = handle.run_on_main_thread(move || {
                    if let Err(err) = show_main_window(&window_handle) {
                        log_palette_warning("failed to show main window on launch", err);
                    }
                }) {
                    log_palette_warning("failed to schedule launch window show", err);
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                if let Err(err) = window.hide() {
                    log_palette_warning("failed to hide main window on close", err);
                }
            }
            WindowEvent::Focused(false) => {
                let app = window.app_handle();
                let blur_dismiss = app.state::<BlurDismiss>().0.load(Ordering::Relaxed);
                if blur_dismiss
                    && merged_settings_or_default(app).hide_on_blur
                    && let Err(err) = window.hide()
                {
                    log_palette_warning("failed to hide main window on focus loss", err);
                }
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .map_err(|err| format!("error while running Axon Palette: {err}").into())
}
