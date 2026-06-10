use super::{infer_content_type, is_structurally_unsafe};

// ── is_structurally_unsafe ────────────────────────────────────────────────────

#[test]
fn safe_relative_path_is_allowed() {
    assert!(!is_structurally_unsafe("screenshots/foo.png"));
}

#[test]
fn bare_filename_is_allowed() {
    assert!(!is_structurally_unsafe("artifact.json"));
}

#[test]
fn parent_dir_component_is_unsafe() {
    assert!(is_structurally_unsafe("../etc/passwd"));
}

#[test]
fn embedded_parent_traversal_is_unsafe() {
    assert!(is_structurally_unsafe("screenshots/../../../etc/passwd"));
}

#[test]
fn absolute_unix_path_is_unsafe() {
    assert!(is_structurally_unsafe("/etc/passwd"));
}

#[test]
fn dotdot_only_is_unsafe() {
    assert!(is_structurally_unsafe(".."));
}

#[test]
fn subdirectory_path_is_safe() {
    assert!(!is_structurally_unsafe("jobs/abc123/output.md"));
}

// ── infer_content_type ────────────────────────────────────────────────────────

#[test]
fn png_returns_image_png() {
    assert_eq!(infer_content_type("shot.png"), "image/png");
}

#[test]
fn jpg_returns_image_jpeg() {
    assert_eq!(infer_content_type("photo.jpg"), "image/jpeg");
}

#[test]
fn jpeg_returns_image_jpeg() {
    assert_eq!(infer_content_type("photo.jpeg"), "image/jpeg");
}

#[test]
fn json_returns_application_json() {
    assert_eq!(infer_content_type("data.json"), "application/json");
}

#[test]
fn md_returns_text_markdown() {
    assert_eq!(
        infer_content_type("README.md"),
        "text/markdown; charset=utf-8"
    );
}

#[test]
fn log_returns_text_plain() {
    assert_eq!(infer_content_type("run.log"), "text/plain; charset=utf-8");
}

#[test]
fn unknown_extension_returns_octet_stream() {
    assert_eq!(
        infer_content_type("archive.tar.gz"),
        "application/octet-stream"
    );
}

#[test]
fn no_extension_returns_octet_stream() {
    assert_eq!(infer_content_type("Makefile"), "application/octet-stream");
}

#[test]
fn uppercase_png_extension_returns_image_png() {
    assert_eq!(infer_content_type("SCREENSHOT.PNG"), "image/png");
}

#[test]
fn svg_returns_image_svg() {
    assert_eq!(infer_content_type("logo.svg"), "image/svg+xml");
}

#[test]
fn html_returns_text_html() {
    assert_eq!(infer_content_type("page.html"), "text/html; charset=utf-8");
}
