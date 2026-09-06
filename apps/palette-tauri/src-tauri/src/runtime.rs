//! Tauri runtime assembly and desktop tray wiring.
//!
//! Keeping the builder and tray lifecycle here prevents `lib.rs` from becoming
//! a monolith while leaving command/configuration logic in the crate root.

use super::*;

#[cfg(desktop)]
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
                    diag::warn_with_context("failed to show main window from tray", err);
                }
            }
            "settings" => {
                if let Err(err) = show_main_window(app) {
                    diag::warn_with_context("failed to show main window for settings", err);
                }
                if let Some(window) = app.get_webview_window("main") {
                    if let Err(err) = window.emit("palette://open-settings", ()) {
                        diag::warn_with_context("failed to emit open settings event", err);
                    }
                } else {
                    diag::warn_with_context("failed to open settings", "main window not found");
                }
            }
            "quit" => {
                if let Err(err) = app.global_shortcut().unregister_all() {
                    diag::warn_with_context("failed to unregister global shortcuts on quit", err);
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

pub(super) fn try_run() -> Result<(), Box<dyn std::error::Error>> {
    let bridge_client = BridgeClient::new()
        .map_err(|err| format!("failed to build HTTP client for Axon bridge: {err}"))?;
    let stream_client = StreamClient::new()
        .map_err(|err| format!("failed to build HTTP client for streaming: {err}"))?;
    let backend_transport = BackendTransport::new()
        .map_err(|err| format!("failed to build multi-backend transport: {err}"))?;
    let github_client = GitHubClient::new()
        .map_err(|err| format!("failed to build HTTP client for GitHub bridge: {err}"))?;

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_opener::init());

    #[cfg(desktop)]
    let builder = builder.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    toggle_main_window(app);
                }
            })
            .build(),
    );

    let builder = builder
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
            axon_bridge::axon_artifact_request,
            backend_transport::backend_http_request,
            backend_transport::backend_http_stream,
            backend_transport::backend_cancel_request,
            save_backend_credential,
            delete_backend_credential,
            axon_http_stream_request,
            browser::browser_open,
            browser::browser_navigate,
            browser::browser_set_bounds,
            browser::browser_back,
            browser::browser_forward,
            browser::browser_reload,
            browser::browser_close,
            github_bridge::github_browse,
            oauth::axon_oauth_login,
            oauth::axon_oauth_logout,
            oauth::axon_oauth_status,
            files_bridge::files_list_dir,
            files_bridge::files_read_file,
            files_bridge::files_read_preview,
            files_bridge::files_write_file,
            files_bridge::files_get_root,
            sftp_bridge::commands::sftp_connect,
            sftp_bridge::commands::sftp_list_dir,
            sftp_bridge::commands::sftp_read_file,
            sftp_bridge::commands::sftp_disconnect,
            sftp_bridge::commands::sftp_list_known_hosts,
            sftp_bridge::commands::sftp_revoke_known_host,
            terminal::terminal_run,
            terminal::terminal_cwd
        ])
        .manage(BlurDismiss(AtomicBool::new(true)))
        .manage(bridge_client)
        .manage(backend_transport)
        .manage(stream_client)
        .manage(github_client)
        .manage(oauth::OauthState::new())
        .manage(sftp_bridge::SftpConnections::new())
        .manage(TerminalState::new());

    #[cfg(desktop)]
    let builder = builder.manage(ActiveShortcut(Mutex::new(None)));

    let builder = builder.setup(|app| {
        #[cfg(desktop)]
        {
            if let Err(err) = install_tray(app) {
                diag::warn_with_context("failed to install tray icon", err);
            }
            let settings = merged_settings_or_default(app.handle());
            register_configured_shortcut(app.handle(), &settings).map_err(anyhow::Error::msg)?;
        }

        let handle = app.handle().clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let window_handle = handle.clone();
            if let Err(err) = handle.run_on_main_thread(move || {
                if let Err(err) = show_main_window(&window_handle) {
                    diag::warn_with_context("failed to show main window on launch", err);
                }
            }) {
                diag::warn_with_context("failed to schedule launch window show", err);
            }
        });
        Ok(())
    });

    #[cfg(desktop)]
    let builder = builder.on_window_event(window_events::handle_window_event);

    builder
        .build(tauri::generate_context!())
        .map_err(|err| format!("error while building Axon Palette: {err}"))?
        .run(|app_handle, event| {
            // Close any still-open SFTP sessions on app exit rather than
            // leaking them (and their underlying SSH channels/sockets).
            if let tauri::RunEvent::Exit = event {
                let connections = app_handle.state::<sftp_bridge::SftpConnections>();
                tauri::async_runtime::block_on(connections.close_all());
            }
        });
    Ok(())
}
