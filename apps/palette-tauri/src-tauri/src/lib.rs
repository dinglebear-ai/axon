use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(desktop)]
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
#[cfg(desktop)]
use tauri::{
    LogicalSize, PhysicalPosition, Position, Size,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
#[cfg(desktop)]
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

mod axon_bridge;
mod backend_credentials;
mod backend_transport;
mod browser;
mod date_math;
mod diag;
mod files_bridge;
mod github_bridge;
mod github_feed;
mod oauth;
mod persistence;
mod runtime;
mod sftp_bridge;
mod sftp_known_hosts;
mod stream;
mod terminal;
#[cfg(desktop)]
mod window_events;

use axon_bridge::{BridgeClient, StreamClient};
use backend_credentials::{delete_backend_credential, save_backend_credential};
use backend_transport::BackendTransport;
use github_bridge::GitHubClient;
use persistence::*;
use stream::axon_http_stream_request;
use terminal::TerminalState;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PaletteSettings {
    server_url: String,
    token: Option<String>,
    #[serde(default)]
    backend_profiles: Vec<BackendProfile>,
    #[serde(default)]
    active_backend_profiles: std::collections::HashMap<BackendProduct, String>,
    shortcut: String,
    collection: String,
    result_limit: u16,
    theme: PaletteTheme,
    hide_on_blur: bool,
    open_results_inline: bool,
    agent_bubbles: bool,
    show_footer_hints: bool,
    /// Persisted SFTP connection profiles (host/username/local-key-path
    /// triples — never a password or key material). See
    /// `persistence::write_settings`'s blast-radius note: settings.json is
    /// tightened to 0600 whenever this is non-empty, since it centralizes
    /// every remote host this palette can reach plus which local key unlocks
    /// each one.
    #[serde(default)]
    sftp_connections: Vec<SftpConnectionProfile>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackendProfile {
    pub id: String,
    pub label: String,
    pub product: BackendProduct,
    pub origin: String,
    pub credential_handle: Option<String>,
    #[serde(default)]
    pub credential_generation: Option<String>,
    pub pinned_server_id: Option<String>,
    pub accepted_api_major: u16,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackendProduct {
    Axon,
    Labby,
    Cortex,
}

/// A persisted SFTP connection profile. Deliberately excludes any password
/// or key *material* — `private_key_path` is a reference to a key file
/// already on disk (see `sftp_bridge::validate_private_key_path`), not the
/// key itself.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SftpConnectionProfile {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub private_key_path: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum PaletteTheme {
    System,
    Dark,
    Light,
}

const DEFAULT_SERVER_URL: &str = "https://axon.example.internal";
const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+Space";
const SETTINGS_FILE: &str = "settings.json";

pub(crate) fn require_desktop_feature(feature: &str) -> Result<(), String> {
    if cfg!(mobile) {
        Err(format!("{feature} is only available in the desktop app"))
    } else {
        Ok(())
    }
}

// Runtime gate for hide-on-blur, toggled by the frontend. The launcher hides on
// blur (click-away dismiss), but while a result/settings view is open we keep it
// up so resizing or copying from another window doesn't make it vanish.
// Checked together with the `hide_on_blur` user preference in the
// `WindowEvent::Focused(false)` handler.
struct BlurDismiss(AtomicBool);

/// Tracks the shortcut label currently registered so we can unregister only
/// that specific shortcut (rather than calling `unregister_all`) when the user
/// changes the keybinding.
#[cfg(desktop)]
struct ActiveShortcut(Mutex<Option<String>>);

#[tauri::command]
fn load_palette_config(app: AppHandle) -> Result<PaletteSettings, String> {
    merged_settings(&app)
}

#[tauri::command]
fn load_palette_default_config() -> PaletteSettings {
    default_settings()
}

#[tauri::command]
fn save_palette_settings(
    app: AppHandle,
    settings: PaletteSettings,
) -> Result<PaletteSettings, String> {
    let settings = normalize_settings(settings);
    // Axon server configuration belongs to the server. The desktop app only
    // persists its connection credentials and palette-local preferences.
    save_palette_prefs(&app, &settings)?;
    update_shortcut(&app, &settings)?;
    Ok(settings)
}

fn save_palette_prefs(app: &AppHandle, settings: &PaletteSettings) -> Result<(), String> {
    write_settings(app, settings).map_err(|err| err.to_string())
}

#[cfg(desktop)]
fn update_shortcut(app: &AppHandle, settings: &PaletteSettings) -> Result<(), String> {
    register_configured_shortcut(app, settings)
}

#[cfg(mobile)]
fn update_shortcut(_app: &AppHandle, _settings: &PaletteSettings) -> Result<(), String> {
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
fn hide_palette(app: AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?
        .hide()
        .map_err(|err| err.to_string())
}

#[cfg(mobile)]
#[tauri::command]
fn hide_palette(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
fn show_palette(app: AppHandle) -> Result<(), String> {
    show_main_window(&app)
}

#[cfg(desktop)]
fn center_position(
    monitor_position: (i32, i32),
    monitor_size: (u32, u32),
    scale_factor: f64,
    window_size: (f64, f64),
) -> (i32, i32) {
    let window_width = window_size.0 * scale_factor;
    let window_height = window_size.1 * scale_factor;
    (
        monitor_position.0 + ((monitor_size.0 as f64 - window_width) / 2.0).round() as i32,
        monitor_position.1 + ((monitor_size.1 as f64 - window_height) / 2.0).round() as i32,
    )
}

#[cfg(desktop)]
fn resize_and_center(window: &tauri::WebviewWindow, width: f64, height: f64) -> Result<(), String> {
    let monitor = window.current_monitor().map_err(|err| err.to_string())?;
    window
        .set_size(Size::Logical(LogicalSize { width, height }))
        .map_err(|err| err.to_string())?;

    if let Some(monitor) = monitor {
        let position = monitor.position();
        let size = monitor.size();
        let (x, y) = center_position(
            (position.x, position.y),
            (size.width, size.height),
            monitor.scale_factor(),
            (width, height),
        );
        window
            .set_position(Position::Physical(PhysicalPosition::new(x, y)))
            .map_err(|err| err.to_string())
    } else {
        window.center().map_err(|err| err.to_string())
    }
}

#[cfg(desktop)]
#[tauri::command]
fn resize_palette(app: AppHandle, width: f64, height: f64, shadow: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    // A maximized window ignores set_size on Windows; drop maximize first so the
    // auto-sizer (and the next launcher open) always lands at the intended size.
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    }
    resize_and_center(&window, width, height)?;
    // Per-view native shadow toggle (see useWindowChrome.ts for the policy).
    let _ = window.set_shadow(shadow);
    Ok(())
}

#[cfg(mobile)]
#[tauri::command]
fn resize_palette(_app: AppHandle, _width: f64, _height: f64, _shadow: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(desktop)]
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

#[cfg(mobile)]
#[tauri::command]
fn toggle_maximize(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
fn set_blur_dismiss(state: tauri::State<'_, BlurDismiss>, enabled: bool) {
    state.0.store(enabled, Ordering::Relaxed);
}

fn merged_settings(app: &AppHandle) -> Result<PaletteSettings, String> {
    let persisted = read_settings_result(app)?;
    let defaults = default_settings();

    Ok(merge_settings(persisted, defaults))
}

#[cfg(desktop)]
fn merged_settings_or_default(app: &AppHandle) -> PaletteSettings {
    match merged_settings(app) {
        Ok(settings) => settings,
        Err(err) => {
            diag::warn(&err.to_string());
            default_settings()
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
        backend_profiles: persisted
            .backend_profiles
            .unwrap_or(defaults.backend_profiles),
        active_backend_profiles: persisted
            .active_backend_profiles
            .unwrap_or(defaults.active_backend_profiles),
        shortcut: persisted
            .shortcut
            .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string()),
        collection: persisted.collection.unwrap_or(defaults.collection),
        result_limit: persisted.result_limit.unwrap_or(10),
        theme: persisted.theme.unwrap_or(PaletteTheme::System),
        hide_on_blur: persisted.hide_on_blur.unwrap_or(true),
        open_results_inline: persisted.open_results_inline.unwrap_or(true),
        agent_bubbles: persisted.agent_bubbles.unwrap_or(false),
        show_footer_hints: persisted.show_footer_hints.unwrap_or(false),
        sftp_connections: persisted.sftp_connections.unwrap_or_default(),
    })
}

fn default_settings() -> PaletteSettings {
    PaletteSettings {
        server_url: DEFAULT_SERVER_URL.to_string(),
        token: None,
        backend_profiles: vec![
            BackendProfile {
                id: "axon-default".to_string(),
                label: "Axon".to_string(),
                product: BackendProduct::Axon,
                origin: DEFAULT_SERVER_URL.to_string(),
                credential_handle: Some("legacy-axon".to_string()),
                credential_generation: None,
                pinned_server_id: None,
                accepted_api_major: 1,
            },
            BackendProfile {
                id: "labby-default".to_string(),
                label: "Labby".to_string(),
                product: BackendProduct::Labby,
                origin: "https://dev.dinglebear.ai".to_string(),
                credential_handle: Some("labby-default-cred".to_string()),
                credential_generation: Some("gen-labby-1".to_string()),
                pinned_server_id: Some("srv-labby".to_string()),
                accepted_api_major: 1,
            },
            BackendProfile {
                id: "cortex-default".to_string(),
                label: "Cortex".to_string(),
                product: BackendProduct::Cortex,
                origin: "https://cortex.tootie.tv".to_string(),
                credential_handle: Some("cortex-default-cred".to_string()),
                credential_generation: Some("gen-cortex-1".to_string()),
                pinned_server_id: Some("srv-cortex".to_string()),
                accepted_api_major: 1,
            },
        ],
        active_backend_profiles: [
            (BackendProduct::Axon, "axon-default".to_string()),
            (BackendProduct::Labby, "labby-default".to_string()),
            (BackendProduct::Cortex, "cortex-default".to_string()),
        ]
        .into_iter()
        .collect(),
        shortcut: DEFAULT_SHORTCUT.to_string(),
        collection: "axon".to_string(),
        result_limit: 10,
        theme: PaletteTheme::System,
        hide_on_blur: true,
        open_results_inline: true,
        agent_bubbles: false,
        show_footer_hints: false,
        sftp_connections: Vec::new(),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialPaletteSettings {
    server_url: Option<String>,
    token: Option<Option<String>>,
    backend_profiles: Option<Vec<BackendProfile>>,
    active_backend_profiles: Option<std::collections::HashMap<BackendProduct, String>>,
    shortcut: Option<String>,
    collection: Option<String>,
    result_limit: Option<u16>,
    theme: Option<PaletteTheme>,
    hide_on_blur: Option<bool>,
    open_results_inline: Option<bool>,
    agent_bubbles: Option<bool>,
    show_footer_hints: Option<bool>,
    sftp_connections: Option<Vec<SftpConnectionProfile>>,
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
    settings.backend_profiles = backend_transport::normalize_profiles(settings.backend_profiles);
    settings.active_backend_profiles.retain(|product, id| {
        settings
            .backend_profiles
            .iter()
            .any(|profile| profile.product == *product && profile.id == *id)
    });
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
        "cmd+space" | "command+space" | "super+space" => "Cmd+Space".to_string(),
        "cmd+shift+space" | "command+shift+space" | "super+shift+space" => {
            "Cmd+Shift+Space".to_string()
        }
        "ctrl+shift+space" | "control+shift+space" => "Ctrl+Shift+Space".to_string(),
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

#[cfg(desktop)]
fn shortcut_for_label(label: &str) -> Shortcut {
    match normalize_shortcut_label(label).as_str() {
        "Alt+Space" => Shortcut::new(Some(Modifiers::ALT), Code::Space),
        "Ctrl+Space" => Shortcut::new(Some(Modifiers::CONTROL), Code::Space),
        "Cmd+Space" => Shortcut::new(Some(Modifiers::SUPER), Code::Space),
        "Cmd+Shift+Space" => Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space),
        _ => Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space),
    }
}

fn shortcut_needs_registration(active_label: Option<&str>, new_label: &str) -> bool {
    active_label != Some(new_label)
}

#[cfg(desktop)]
fn register_configured_shortcut(app: &AppHandle, settings: &PaletteSettings) -> Result<(), String> {
    let new_label = normalize_shortcut_label(&settings.shortcut);
    let new_shortcut = shortcut_for_label(&new_label);

    // Unregister only the previously registered shortcut if we know what it is,
    // rather than calling `unregister_all` which would also unregister shortcuts
    // registered by other parts of the app.
    if let Ok(mut guard) = app.state::<ActiveShortcut>().0.lock() {
        if !shortcut_needs_registration(guard.as_deref(), &new_label) {
            return Ok(());
        }
        if let Some(old_label) = guard.take().filter(|l| l != &new_label) {
            let old_shortcut = shortcut_for_label(&old_label);
            if let Err(err) = app.global_shortcut().unregister(old_shortcut) {
                diag::warn(&format!(
                    "failed to unregister old shortcut '{old_label}': {err}"
                ));
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

#[cfg(desktop)]
fn show_main_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    }
    // Compact launcher — matches COMPACT in useWindowChrome.ts (bar + inset).
    resize_and_center(&window, 720.0, 92.0)?;
    // Compact floats a CSS-glowing bar; keep the native shadow off (JS re-asserts).
    let _ = window.set_shadow(false);
    window.show().map_err(|err| err.to_string())?;
    if let Ok(true) = window.is_minimized() {
        window.unminimize().map_err(|err| err.to_string())?;
    }
    window.set_focus().map_err(|err| err.to_string())?;
    if let Err(err) = window.emit("palette://shown", ()) {
        diag::warn_with_context("failed to emit shown event", err);
    }
    Ok(())
}

#[cfg(mobile)]
fn show_main_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.show().map_err(|err| err.to_string())?;
    if let Err(err) = window.emit("palette://shown", ()) {
        diag::warn_with_context("failed to emit shown event", err);
    }
    Ok(())
}

#[cfg(desktop)]
fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let is_visible = window.is_visible().unwrap_or(false);
    let is_focused = window.is_focused().unwrap_or(false);
    if is_visible && is_focused {
        if let Err(err) = window.hide() {
            diag::warn_with_context("failed to hide main window", err);
        }
    } else {
        if let Err(err) = show_main_window(app) {
            diag::warn_with_context("failed to show main window", err);
        }
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(mobile)]
#[tauri::mobile_entry_point]
pub fn run() {
    runtime::try_run().expect("error while running Axon Palette");
}

#[cfg(desktop)]
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    runtime::try_run()
}
