//! Render mode projections at the provider boundary.
use axon_api::source::RenderMode;
use axon_core::config::RenderMode as CoreRenderMode;

pub(crate) fn map_render_mode(mode: RenderMode) -> CoreRenderMode {
    match mode {
        RenderMode::Http => CoreRenderMode::Http,
        RenderMode::Chrome => CoreRenderMode::Chrome,
        RenderMode::AutoSwitch => CoreRenderMode::AutoSwitch,
    }
}

pub(crate) fn map_core_render_mode(mode: CoreRenderMode) -> RenderMode {
    match mode {
        CoreRenderMode::Http => RenderMode::Http,
        CoreRenderMode::Chrome => RenderMode::Chrome,
        CoreRenderMode::AutoSwitch => RenderMode::AutoSwitch,
    }
}
