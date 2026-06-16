use super::{
    artifact_headers_for_path, is_structurally_unsafe, open_artifact_error, resolve_artifact_path,
};
use axum::http::StatusCode;

#[test]
fn safe_relative_artifact_paths_are_allowed() {
    for path in [
        "screenshots/foo.png",
        "artifact.json",
        "jobs/abc123/output.md",
    ] {
        assert!(!is_structurally_unsafe(path), "{path}");
    }
}

#[test]
fn unsafe_artifact_paths_are_rejected_structurally() {
    for path in [
        "",
        "..",
        "../secret.txt",
        "/etc/passwd",
        "screenshots/../../../etc/passwd",
        "screenshots/../secret.txt",
        "screenshots/%2e%2e/secret.txt",
        "screenshots/%2e/secret.txt",
        r"screenshots\\..\\secret.txt",
        r"screenshots\\shot.png",
        "screenshots%5cshot.png",
        r"C:\\Windows\\secret.txt",
        "screenshots/shot.png\0",
    ] {
        assert!(is_structurally_unsafe(path), "{path:?}");
    }
}

#[test]
fn raster_images_are_inline_but_active_content_is_attachment() {
    assert_eq!(
        artifact_headers_for_path("screenshots/shot.png").content_type,
        "image/png"
    );
    assert!(
        artifact_headers_for_path("screenshots/shot.png")
            .content_disposition
            .is_none()
    );
    assert_eq!(
        artifact_headers_for_path("page.html").content_type,
        "application/octet-stream"
    );
    assert!(
        artifact_headers_for_path("page.html")
            .content_disposition
            .unwrap()
            .starts_with("attachment")
    );
    assert_eq!(
        artifact_headers_for_path("logo.svg").content_type,
        "application/octet-stream"
    );
}

#[tokio::test]
async fn symlink_component_under_output_root_is_forbidden() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("output");
    let screenshots = root.join("screenshots");
    tokio::fs::create_dir_all(&screenshots).await.unwrap();
    tokio::fs::write(screenshots.join("real.png"), b"png")
        .await
        .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(screenshots.join("real.png"), screenshots.join("alias.png"))
        .unwrap();
    #[cfg(unix)]
    {
        let realdir = root.join("realdir");
        tokio::fs::create_dir_all(&realdir).await.unwrap();
        tokio::fs::write(realdir.join("nested.png"), b"png")
            .await
            .unwrap();
        std::os::unix::fs::symlink(&realdir, screenshots.join("linkdir")).unwrap();
    }

    #[cfg(unix)]
    {
        let err = resolve_artifact_path(&root, "screenshots/alias.png")
            .await
            .expect_err("symlink should be rejected");
        assert_eq!(err.status(), StatusCode::FORBIDDEN);

        let err = resolve_artifact_path(&root, "screenshots/linkdir/nested.png")
            .await
            .expect_err("intermediate symlink directory should be rejected");
        assert_eq!(err.status(), StatusCode::FORBIDDEN);
    }
}

#[test]
fn artifact_content_types_are_inferred_from_extension() {
    for (path, content_type) in [
        ("shot.png", "image/png"),
        ("photo.jpg", "image/jpeg"),
        ("photo.jpeg", "image/jpeg"),
        ("data.json", "application/json"),
        ("README.md", "text/markdown; charset=utf-8"),
        ("run.log", "text/plain; charset=utf-8"),
        ("archive.tar.gz", "application/octet-stream"),
        ("Makefile", "application/octet-stream"),
        ("SCREENSHOT.PNG", "image/png"),
        ("logo.svg", "application/octet-stream"),
        ("page.html", "application/octet-stream"),
    ] {
        assert_eq!(artifact_headers_for_path(path).content_type, content_type);
    }

    assert!(
        artifact_headers_for_path("data.json")
            .content_disposition
            .unwrap()
            .starts_with("attachment")
    );
}

#[test]
fn open_error_maps_vanished_file_to_404_and_other_io_errors_to_500() {
    // A file validated by resolve_artifact_path can vanish before File::open in
    // the TOCTOU window — that is a 404, not a server error.
    let not_found = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
    assert_eq!(
        open_artifact_error(&not_found, "jobs/abc/output.md").status(),
        StatusCode::NOT_FOUND
    );

    // Any other IO error (e.g. permission denied) is a genuine 500.
    let denied = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
    assert_eq!(
        open_artifact_error(&denied, "jobs/abc/output.md").status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn download_filename_strips_header_injection_characters() {
    // A double-quote is not rejected by `is_structurally_unsafe`, so it can reach
    // the Content-Disposition header; CR/LF would split headers. Both must be
    // sanitized to `_` and only the leaf name should appear.
    let disposition = artifact_headers_for_path("jobs/abc/re\"port\r\n.json")
        .content_disposition
        .expect("non-inline type should have a disposition");
    assert_eq!(disposition, "attachment; filename=\"re_port__.json\"");

    // The directory prefix must not leak into the filename.
    let nested = artifact_headers_for_path("jobs/abc123/output.log")
        .content_disposition
        .expect("log is non-inline");
    assert_eq!(nested, "attachment; filename=\"output.log\"");

    let unicode = artifact_headers_for_path("jobs/abc/résumé\t.json")
        .content_disposition
        .expect("json is non-inline");
    assert_eq!(unicode, "attachment; filename=\"r_sum__.json\"");
}
