use std::sync::atomic::Ordering;

use tauri::{Manager, Window, WindowEvent};

use super::{BlurDismiss, log_palette_warning, merged_settings_or_default};

pub(super) fn handle_window_event(window: &Window, event: &WindowEvent) {
    match event {
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
    }
}
