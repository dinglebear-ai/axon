// Real in-app "Browser" tool: an embedded native `Webview` child attached
// directly inside the main palette window (Tauri v2 multiwebview), rendering
// live external web pages inside the `.browser-surface` area.
//
// Benefits of a native child webview vs an iframe or separate window:
// - Arbitrary external sites (Google, Unraid, GitHub, etc.) load natively
//   without being blocked by X-Frame-Options: DENY or CSP frame-ancestors.
// - The browser renders seamlessly inside the palette window underneath the
//   custom tabstrip and address bar without opening a disconnected OS window.
// - The child webview position and size track `.browser-surface` via
//   `browser_set_bounds` dynamically.
use serde::Deserialize;
use tauri::{AppHandle, Manager, WebviewUrl};
#[cfg(desktop)]
use tauri::{LogicalPosition, LogicalSize, Position, Size, WebviewBuilder};

/// Label of the embedded child browser webview attached to the main window.
pub(crate) const BROWSER_WEBVIEW_LABEL: &str = "browser_content";

#[derive(Debug, Clone, Copy, Deserialize)]
pub(crate) struct BrowserBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Validate and normalize a URL for the browser window. Rejects anything
/// that isn't `http`/`https`/`about:blank` so the browser command surface
/// can't be used to load `file://`/`tauri://`/custom-scheme URLs into a
/// window that otherwise behaves like a sandboxed external browser.
///
/// Returns the validated URL string (unchanged) on success.
pub(crate) fn validate_browser_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("browser URL must not be empty".to_string());
    }
    if trimmed == "about:blank" {
        return Ok(trimmed.to_string());
    }
    let parsed = url::Url::parse(trimmed).map_err(|err| format!("invalid browser URL: {err}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("browser URL must use http or https".to_string());
    }
    Ok(trimmed.to_string())
}

fn webview_url_for(raw: &str) -> Result<WebviewUrl, String> {
    if raw == "about:blank" {
        return Ok(WebviewUrl::App("about:blank".into()));
    }
    let parsed = url::Url::parse(raw).map_err(|err| format!("invalid browser URL: {err}"))?;
    Ok(WebviewUrl::External(parsed))
}

/// Open or navigate the embedded child browser webview inside the main palette
/// window at `url`.
#[tauri::command]
pub(crate) async fn browser_open(
    app: AppHandle,
    url: String,
    bounds: Option<BrowserBounds>,
) -> Result<(), String> {
    crate::require_desktop_feature("Browser")?;
    #[cfg(mobile)]
    {
        let _ = (app, url, bounds);
        return Err("Browser is only available in the desktop app".to_string());
    }

    #[cfg(desktop)]
    {
        let validated = validate_browser_url(&url)?;
        let parsed_url =
            url::Url::parse(&validated).map_err(|err| format!("invalid browser URL: {err}"))?;

        if let Some(webview) = app.get_webview(BROWSER_WEBVIEW_LABEL) {
            if let Some(b) = bounds {
                let _ = webview.set_position(Position::Logical(LogicalPosition::new(b.x, b.y)));
                let _ = webview.set_size(Size::Logical(LogicalSize::new(b.width, b.height)));
            }
            let _ = webview.show();
            webview
                .navigate(parsed_url)
                .map_err(|err| err.to_string())?;
            let _ = webview.set_focus();
            return Ok(());
        }

        let main_window = app.get_window("main").ok_or("main window not found")?;
        let webview_url = webview_url_for(&validated)?;
        let builder = WebviewBuilder::new(BROWSER_WEBVIEW_LABEL, webview_url);

        let (pos, size) = if let Some(b) = bounds.filter(|b| b.width > 0.0 && b.height > 0.0) {
            (
                LogicalPosition::new(b.x, b.y),
                LogicalSize::new(b.width, b.height),
            )
        } else {
            (
                LogicalPosition::new(0.0, 80.0),
                LogicalSize::new(1280.0, 780.0),
            )
        };

        main_window
            .add_child(builder, pos, size)
            .map_err(|err| err.to_string())?;

        if let Some(webview) = app.get_webview(BROWSER_WEBVIEW_LABEL) {
            let _ = webview.set_focus();
        }
        Ok(())
    }
}

/// Navigate the existing embedded browser to a new URL. Opens/attaches it
/// first if it isn't already attached.
#[tauri::command]
pub(crate) async fn browser_navigate(
    app: AppHandle,
    url: String,
    bounds: Option<BrowserBounds>,
) -> Result<(), String> {
    browser_open(app, url, bounds).await
}

/// Update the bounds (position and size in logical pixels relative to main window)
/// of the embedded browser child webview.
#[tauri::command]
pub(crate) fn browser_set_bounds(app: AppHandle, bounds: BrowserBounds) -> Result<(), String> {
    crate::require_desktop_feature("Browser")?;
    #[cfg(mobile)]
    {
        let _ = (app, bounds);
        return Err("Browser is only available in the desktop app".to_string());
    }

    #[cfg(desktop)]
    {
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return Ok(());
        }
        if let Some(webview) = app.get_webview(BROWSER_WEBVIEW_LABEL) {
            webview
                .set_position(Position::Logical(LogicalPosition::new(bounds.x, bounds.y)))
                .map_err(|err| err.to_string())?;
            webview
                .set_size(Size::Logical(LogicalSize::new(bounds.width, bounds.height)))
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    }
}

/// Drive the loaded page's own back-navigation. A no-op (not an error) if
/// the browser isn't currently open.
#[tauri::command]
pub(crate) fn browser_back(app: AppHandle) -> Result<(), String> {
    crate::require_desktop_feature("Browser")?;
    with_browser_webview(&app, |webview| {
        webview
            .eval("history.back()")
            .map_err(|err| err.to_string())
    })
}

/// Drive the loaded page's own forward-navigation.
#[tauri::command]
pub(crate) fn browser_forward(app: AppHandle) -> Result<(), String> {
    crate::require_desktop_feature("Browser")?;
    with_browser_webview(&app, |webview| {
        webview
            .eval("history.forward()")
            .map_err(|err| err.to_string())
    })
}

/// Reload the currently loaded page.
#[tauri::command]
pub(crate) fn browser_reload(app: AppHandle) -> Result<(), String> {
    crate::require_desktop_feature("Browser")?;
    with_browser_webview(&app, |webview| {
        webview
            .eval("location.reload()")
            .map_err(|err| err.to_string())
    })
}

/// Close (destroy) the embedded child browser webview if it exists.
#[tauri::command]
pub(crate) fn browser_close(app: AppHandle) -> Result<(), String> {
    crate::require_desktop_feature("Browser")?;
    #[cfg(mobile)]
    {
        let _ = app;
        return Err("Browser is only available in the desktop app".to_string());
    }

    #[cfg(desktop)]
    {
        if let Some(webview) = app.get_webview(BROWSER_WEBVIEW_LABEL) {
            webview.close().map_err(|err| err.to_string())?;
        }
        Ok(())
    }
}

fn with_browser_webview(
    app: &AppHandle,
    f: impl FnOnce(&tauri::Webview) -> Result<(), String>,
) -> Result<(), String> {
    match app.get_webview(BROWSER_WEBVIEW_LABEL) {
        Some(webview) => f(&webview),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
