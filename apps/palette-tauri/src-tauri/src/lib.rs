use std::{fs, path::PathBuf};

use serde::Serialize;
use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PaletteConfig {
    server_url: String,
    token: Option<String>,
}

const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8001";

#[tauri::command]
fn load_palette_config() -> PaletteConfig {
    let env_entries = read_default_env_entries();
    let server_url = value_for("AXON_SERVER_URL", &env_entries)
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string());
    let token = value_for("AXON_MCP_HTTP_TOKEN", &env_entries)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    PaletteConfig { server_url, token }
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

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_http::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![load_palette_config])
        .setup(|app| {
            let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
            app.global_shortcut().register(shortcut)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                let _ = window.app_handle().global_shortcut().unregister_all();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Axon Palette");
}
