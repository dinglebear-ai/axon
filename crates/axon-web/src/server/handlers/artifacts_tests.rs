use super::{ArtifactContentQuery, artifact_content_response, open_artifact_error};
use axon_api::source::ArtifactId;
use axon_services::artifacts::ArtifactContentFile;
use axum::body::to_bytes;
use axum::http::StatusCode;

#[test]
fn open_error_maps_missing_content_to_404_and_other_io_errors_to_500() {
    let not_found = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
    assert_eq!(
        open_artifact_error(&not_found, "art_report_123").status(),
        StatusCode::NOT_FOUND
    );

    let denied = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
    assert_eq!(
        open_artifact_error(&denied, "art_report_123").status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn opaque_artifact_content_is_streamed_with_a_bounded_content_length() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("art_raw_test.bin");
    tokio::fs::write(&path, b"stream me").await.unwrap();
    let response = artifact_content_response(
        ArtifactContentFile {
            artifact_id: ArtifactId::new("art_raw_test"),
            content_type: "application/octet-stream".to_string(),
            disposition: "attachment; filename=\"test.bin\"".to_string(),
            size_bytes: 9,
            path,
        },
        ArtifactContentQuery { download: true },
    )
    .await
    .unwrap();

    assert_eq!(response.headers()["content-length"], "9");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        to_bytes(response.into_body(), 9).await.unwrap().as_ref(),
        b"stream me"
    );
}
