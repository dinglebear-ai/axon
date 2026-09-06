use tauri::{AppHandle, Manager};
#[cfg(desktop)]
use tauri::{LogicalSize, PhysicalPosition, Position, Size};

#[cfg(desktop)]
#[tauri::command]
pub(super) fn hide_palette(app: AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?
        .hide()
        .map_err(|err| err.to_string())
}

#[cfg(mobile)]
#[tauri::command]
pub(super) fn hide_palette(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub(super) fn show_palette(app: AppHandle) -> Result<(), String> {
    super::show_main_window(&app)
}

#[cfg(desktop)]
pub(super) fn center_position(
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
pub(super) fn resize_and_center(
    window: &tauri::WebviewWindow,
    width: f64,
    height: f64,
) -> Result<(), String> {
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
pub(super) fn resize_palette(
    app: AppHandle,
    width: f64,
    height: f64,
    shadow: bool,
) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    }
    resize_and_center(&window, width, height)?;
    let _ = window.set_shadow(shadow);
    Ok(())
}

#[cfg(mobile)]
#[tauri::command]
pub(super) fn resize_palette(
    _app: AppHandle,
    _width: f64,
    _height: f64,
    _shadow: bool,
) -> Result<(), String> {
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
pub(super) fn toggle_maximize(app: AppHandle) -> Result<(), String> {
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
pub(super) fn toggle_maximize(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
#[path = "window_commands_tests.rs"]
mod tests;
