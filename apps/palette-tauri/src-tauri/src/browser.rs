// Real in-app "Browser" tool: a dedicated native `WebviewWindow`, separate
// from the main palette window, that navigates to real external URLs.
//
// Why a separate native window instead of an embedded child webview:
// - The main window's CSP (`tauri.conf.json` -> app.security.csp) is locked
//   to `default-src 'self'` with no `frame-src` allowance, so a plain
//   `<iframe src="https://...">` inside the main webview cannot load
//   third-party sites.
// - Tauri v2 does support embedding a child `Webview` inside an existing
//   window (multiwebview), but that requires manually keeping the child
//   webview's bounds in sync with the main window's geometry, which is
//   already tightly hand-managed for the compact/expanded palette states
//   (see `resize_palette` in `lib.rs` and `useWindowChrome.ts` on the
//   frontend). A second bounds-tracking system for a browser pane adds
//   fragility for comparatively little visual benefit.
// - A dedicated `WebviewWindow` is a fully real, independently-navigable
//   webview with its own security context (not bound by the main window's
//   CSP), and it is trivial to create, show, resize, and destroy without
//   touching the main window's carefully-tuned geometry code at all.
//
// Tauri v2 does not expose a first-class "go back"/"go forward" API on
// `WebviewWindow`, so those two commands drive the loaded page's own
// session history via `eval("history.back()")` / `eval("history.forward()")`
// — which is a real navigation of whatever real page is currently loaded,
// not a simulation.
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Label of the dedicated browser window. Distinct from the main palette
/// window's `"main"` label.
pub(crate) const BROWSER_WINDOW_LABEL: &str = "browser";

const BROWSER_WINDOW_TITLE: &str = "Axon Browser";
const DEFAULT_WIDTH: f64 = 1100.0;
const DEFAULT_HEIGHT: f64 = 760.0;

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

/// Open the browser window at `url`, creating it if it doesn't exist yet, or
/// focusing + navigating the existing one.
///
/// Tauri v2 documents a Windows-specific deadlock when creating a
/// `WebviewWindow` synchronously inside a command handler, so this command
/// is `async` (Tauri runs async commands on a separate thread pool).
#[tauri::command]
pub(crate) async fn browser_open(app: AppHandle, url: String) -> Result<(), String> {
    let validated = validate_browser_url(&url)?;
    if let Some(window) = app.get_webview_window(BROWSER_WINDOW_LABEL) {
        window
            .navigate(
                url::Url::parse(&validated).map_err(|err| format!("invalid browser URL: {err}"))?,
            )
            .map_err(|err| err.to_string())?;
        window.show().map_err(|err| err.to_string())?;
        window.set_focus().map_err(|err| err.to_string())?;
        return Ok(());
    }

    let webview_url = webview_url_for(&validated)?;
    WebviewWindowBuilder::new(&app, BROWSER_WINDOW_LABEL, webview_url)
        .title(BROWSER_WINDOW_TITLE)
        .inner_size(DEFAULT_WIDTH, DEFAULT_HEIGHT)
        .center()
        .resizable(true)
        .build()
        .map_err(|err| err.to_string())?;
    Ok(())
}

/// Navigate the existing browser window to a new URL. Opens the window first
/// if it isn't already open (same behavior as `browser_open`).
#[tauri::command]
pub(crate) async fn browser_navigate(app: AppHandle, url: String) -> Result<(), String> {
    browser_open(app, url).await
}

/// Drive the loaded page's own back-navigation. A no-op (not an error) if
/// the browser window isn't currently open.
#[tauri::command]
pub(crate) fn browser_back(app: AppHandle) -> Result<(), String> {
    with_browser_window(&app, |window| {
        window.eval("history.back()").map_err(|err| err.to_string())
    })
}

/// Drive the loaded page's own forward-navigation.
#[tauri::command]
pub(crate) fn browser_forward(app: AppHandle) -> Result<(), String> {
    with_browser_window(&app, |window| {
        window
            .eval("history.forward()")
            .map_err(|err| err.to_string())
    })
}

/// Reload the currently loaded page.
#[tauri::command]
pub(crate) fn browser_reload(app: AppHandle) -> Result<(), String> {
    with_browser_window(&app, |window| {
        window
            .eval("location.reload()")
            .map_err(|err| err.to_string())
    })
}

/// Close (destroy) the browser window if it exists.
#[tauri::command]
pub(crate) fn browser_close(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(BROWSER_WINDOW_LABEL) {
        window.close().map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn with_browser_window(
    app: &AppHandle,
    f: impl FnOnce(&tauri::WebviewWindow) -> Result<(), String>,
) -> Result<(), String> {
    match app.get_webview_window(BROWSER_WINDOW_LABEL) {
        Some(window) => f(&window),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
