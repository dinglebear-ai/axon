use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Size, WindowEvent,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

mod axon_bridge;

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
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum PaletteTheme {
    System,
    Dark,
    Light,
}

const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8001";
const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+Space";
const SETTINGS_FILE: &str = "settings.json";

#[tauri::command]
fn load_palette_config(app: AppHandle) -> Result<PaletteSettings, String> {
    merged_settings(&app)
}

#[tauri::command]
fn save_palette_settings(
    app: AppHandle,
    settings: PaletteSettings,
) -> Result<PaletteSettings, String> {
    let settings = normalize_settings(settings);
    write_settings(&app, &settings).map_err(|err| err.to_string())?;
    register_configured_shortcut(&app, &settings).map_err(|err| err.to_string())?;
    Ok(settings)
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
    window
        .set_size(Size::Logical(LogicalSize { width, height }))
        .map_err(|err| err.to_string())?;
    window.center().map_err(|err| err.to_string())
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
            .or_else(|| Some(defaults.server_url))
            .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string()),
        token: persisted.token.unwrap_or(defaults.token),
        shortcut: persisted
            .shortcut
            .or_else(|| Some(DEFAULT_SHORTCUT.to_string()))
            .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string()),
        collection: persisted.collection.unwrap_or(defaults.collection),
        result_limit: persisted.result_limit.unwrap_or(10),
        theme: persisted.theme.unwrap_or(PaletteTheme::System),
        hide_on_blur: persisted.hide_on_blur.unwrap_or(true),
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
}

fn read_settings_result(app: &AppHandle) -> Result<PartialPaletteSettings, String> {
    let Some(path) = settings_path(app) else {
        return Ok(PartialPaletteSettings::default());
    };
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Ok(PartialPaletteSettings::default());
        }
        Err(err) => {
            return Err(format!(
                "failed to read palette settings at {}: {err}",
                path.display()
            ));
        }
    };
    parse_settings_json(&contents, &path)
}

fn parse_settings_json(contents: &str, path: &Path) -> Result<PartialPaletteSettings, String> {
    serde_json::from_str(contents).map_err(|err| {
        format!(
            "failed to parse palette settings at {}: {err}",
            path.display()
        )
    })
}

fn write_settings(
    app: &AppHandle,
    settings: &PaletteSettings,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = settings_path(app).ok_or("settings path unavailable")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join(SETTINGS_FILE))
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

fn shortcut_for_label(label: &str) -> Shortcut {
    match normalize_shortcut_label(label).as_str() {
        "Alt+Space" => Shortcut::new(Some(Modifiers::ALT), Code::Space),
        "Ctrl+Space" => Shortcut::new(Some(Modifiers::CONTROL), Code::Space),
        "Cmd+Shift+Space" => Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space),
        _ => Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space),
    }
}

fn register_configured_shortcut(app: &AppHandle, settings: &PaletteSettings) -> Result<(), String> {
    let shortcut = shortcut_for_label(&settings.shortcut);
    app.global_shortcut()
        .unregister_all()
        .map_err(|err| err.to_string())?;
    app.global_shortcut()
        .register(shortcut)
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn show_main_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.center().map_err(|err| err.to_string())?;
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    let _ = window.emit("palette://shown", ());
    Ok(())
}

fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    match window.is_visible() {
        Ok(true) => {
            let _ = window.hide();
        }
        _ => {
            let _ = show_main_window(app);
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
                let _ = show_main_window(app);
            }
            "settings" => {
                let _ = show_main_window(app);
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("palette://open-settings", ());
                }
            }
            "quit" => {
                let _ = app.global_shortcut().unregister_all();
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

fn value_for(key: &str, file_entries: &[(String, String)]) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            file_entries
                .iter()
                .find(|(entry_key, _)| entry_key == key)
                .map(|(_, value)| value.clone())
        })
}

fn read_default_env_entries() -> Vec<(String, String)> {
    let Some(path) = default_env_path() else {
        return Vec::new();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_env_entries(&contents)
}

fn default_env_path() -> Option<PathBuf> {
    std::env::var_os("AXON_ENV_PATH")
        .or_else(|| std::env::var_os("AXON_ENV_FILE"))
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".axon/.env")))
}

fn parse_env_entries(contents: &str) -> Vec<(String, String)> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            Some((key.to_string(), trim_env_value(value)))
        })
        .collect()
}

fn trim_env_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_settings_uses_default_collection_when_persisted_collection_missing() {
        let defaults = default_settings(&[("AXON_COLLECTION".to_string(), "docs".to_string())]);

        let merged = merge_settings(PartialPaletteSettings::default(), defaults);

        assert_eq!(merged.collection, "docs");
        assert!(merged.hide_on_blur);
    }

    #[test]
    fn merge_settings_keeps_persisted_collection_over_default() {
        let defaults = default_settings(&[("AXON_COLLECTION".to_string(), "docs".to_string())]);
        let persisted = PartialPaletteSettings {
            collection: Some("saved".to_string()),
            ..PartialPaletteSettings::default()
        };

        let merged = merge_settings(persisted, defaults);

        assert_eq!(merged.collection, "saved");
    }

    #[test]
    fn parse_settings_json_reports_path_on_malformed_settings() {
        let path = Path::new("/tmp/axon-palette/settings.json");
        let err = parse_settings_json("{not json", path).expect_err("malformed settings fail");

        assert!(err.contains("/tmp/axon-palette/settings.json"));
        assert!(err.contains("failed to parse palette settings"));
    }
}

pub fn run() {
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
            save_palette_settings,
            hide_palette,
            show_palette,
            resize_palette,
            axon_bridge::axon_http_request
        ])
        .setup(|app| {
            let _ = install_tray(app);
            let settings = merged_settings_or_default(app.handle());
            register_configured_shortcut(app.handle(), &settings).map_err(anyhow::Error::msg)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Focused(false) => {
                if merged_settings_or_default(window.app_handle()).hide_on_blur {
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running Axon Palette");
}
