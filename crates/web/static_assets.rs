use axum::{
    body::Body,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "apps/web/out/"]
struct WebAssets;

pub(crate) async fn serve_static(uri: axum::http::Uri) -> Response {
    let path = normalize_asset_path(uri.path());
    match asset_response(&path).or_else(|| asset_response("index.html")) {
        Some(response) => response,
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

fn normalize_asset_path(path: &str) -> String {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return "index.html".to_string();
    }
    if path.ends_with('/') {
        return format!("{path}index.html");
    }
    path.to_string()
}

fn asset_response(path: &str) -> Option<Response> {
    let asset = WebAssets::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(asset.data.into_owned()))
        .ok()
}
