use crate::types::ClientActionError;
use axon_api::mcp_schema::McpRenderMode;
use axon_core::config::RenderMode;

pub(super) fn map_render_mode(mode: McpRenderMode) -> RenderMode {
    match mode {
        McpRenderMode::Http => RenderMode::Http,
        McpRenderMode::Chrome => RenderMode::Chrome,
        McpRenderMode::AutoSwitch => RenderMode::AutoSwitch,
    }
}

pub(super) fn parse_viewport(
    raw: Option<&str>,
    fallback_width: u32,
    fallback_height: u32,
) -> Result<(u32, u32), ClientActionError> {
    let Some(raw) = raw else {
        return Ok((fallback_width, fallback_height));
    };
    let Some((width, height)) = raw.split_once('x') else {
        return Err(ClientActionError::new(
            "invalid_request",
            format!("invalid viewport '{raw}': expected WxH"),
            false,
            None,
        ));
    };
    let width = width.parse::<u32>().map_err(|err| {
        ClientActionError::new(
            "invalid_request",
            format!("invalid viewport width '{width}': {err}"),
            false,
            None,
        )
    })?;
    let height = height.parse::<u32>().map_err(|err| {
        ClientActionError::new(
            "invalid_request",
            format!("invalid viewport height '{height}': {err}"),
            false,
            None,
        )
    })?;
    if width == 0 || height == 0 {
        return Err(ClientActionError::new(
            "invalid_request",
            "viewport width and height must be greater than zero",
            false,
            None,
        ));
    }
    Ok((width, height))
}
