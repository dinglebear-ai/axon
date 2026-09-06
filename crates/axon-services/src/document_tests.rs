use super::{
    decode_document_cursor_backend, is_stale, paginate_document, read_latest_stored_source,
};
use crate::types::{DocumentBackend, PagedDocument};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

#[test]
fn paginate_document_obeys_default_budget_boundary() {
    let content = "a".repeat(PagedDocument::DEFAULT_TOKEN_BUDGET * PagedDocument::CHARS_PER_TOKEN);
    let page =
        paginate_document(&content, None, None, DocumentBackend::Qdrant).expect("page document");
    assert_eq!(page.content.len(), content.len());
    assert!(!page.truncated);
    assert_eq!(
        page.token_estimate,
        Some(PagedDocument::DEFAULT_TOKEN_BUDGET)
    );
    assert!(page.next_cursor.is_none());
}

#[test]
fn paginate_document_returns_opaque_continuation_cursor() {
    let content =
        "a".repeat(PagedDocument::DEFAULT_TOKEN_BUDGET * PagedDocument::CHARS_PER_TOKEN + 11);
    let first =
        paginate_document(&content, None, None, DocumentBackend::StoredSource).expect("first page");
    assert!(first.truncated);
    let cursor = first
        .next_cursor
        .as_deref()
        .expect("continuation cursor should exist");
    let second = paginate_document(&content, Some(cursor), None, DocumentBackend::StoredSource)
        .expect("second page");
    assert_eq!(second.content, "a".repeat(11));
    assert!(!second.truncated);
    assert!(second.next_cursor.is_none());
}

#[test]
fn document_cursor_rejects_tampering_staleness_bounds_and_unknown_versions() {
    let content = "abcdefghij";
    let first = paginate_document(content, None, Some(1), DocumentBackend::StoredSource)
        .expect("first page");
    let cursor = first.next_cursor.expect("cursor");

    let mut decoded: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&cursor).expect("base64 cursor"))
            .expect("cursor json");
    decoded["offset"] = serde_json::json!(9);
    let tampered = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&decoded).unwrap());
    assert!(
        paginate_document(
            content,
            Some(&tampered),
            Some(1),
            DocumentBackend::StoredSource
        )
        .unwrap_err()
        .to_string()
        .contains("checksum")
    );

    assert!(
        paginate_document(
            "changed",
            Some(&cursor),
            Some(1),
            DocumentBackend::StoredSource
        )
        .unwrap_err()
        .to_string()
        .contains("stale")
    );

    decoded["version"] = serde_json::json!(2);
    let unknown = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&decoded).unwrap());
    assert!(
        paginate_document(
            content,
            Some(&unknown),
            Some(1),
            DocumentBackend::StoredSource
        )
        .unwrap_err()
        .to_string()
        .contains("version")
    );

    let out_of_bounds = super::encode_document_cursor(&super::DocumentCursor::new(
        DocumentBackend::StoredSource,
        content,
        content.len() + 1,
    ))
    .expect("cursor");
    assert!(
        paginate_document(
            content,
            Some(&out_of_bounds),
            Some(1),
            DocumentBackend::StoredSource
        )
        .unwrap_err()
        .to_string()
        .contains("bounds")
    );
}

#[test]
fn document_cursor_continuation_is_exact_for_multibyte_content() {
    let content = "ééééé";
    let mut cursor = None;
    let mut reconstructed = String::new();
    loop {
        let page = paginate_document(
            content,
            cursor.as_deref(),
            Some(1),
            DocumentBackend::StoredSource,
        )
        .unwrap();
        reconstructed.push_str(&page.content);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(reconstructed, content);
}

#[test]
fn paginate_document_tolerates_non_char_boundary_cursor() {
    let content = "éclair";
    let page = PagedDocument::from_full_content(content, 1, Some(1), DocumentBackend::StoredSource);

    assert_eq!(page.content, "clai");
    assert!(page.truncated);
    assert_eq!(page.next_cursor.as_deref(), Some("6"));
}

#[test]
fn decode_document_cursor_backend_returns_expected_backend() {
    let content = "hello world";
    let page =
        paginate_document(content, None, Some(1), DocumentBackend::LiveScrape).expect("paginated");
    let backend = decode_document_cursor_backend(page.next_cursor.as_deref())
        .expect("decode cursor")
        .expect("backend");
    assert_eq!(backend, DocumentBackend::LiveScrape);
}

#[tokio::test]
async fn read_latest_stored_source_prefers_newest_match() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();
    let old_dir = root.join("older");
    let new_dir = root.join("newer");
    std::fs::create_dir_all(&old_dir).expect("older dir");
    std::fs::create_dir_all(&new_dir).expect("newer dir");
    let file_name = axon_core::content::url_to_filename("https://example.com/docs", 1);
    let suffix: String = file_name.chars().skip(4).collect();
    let old_path = old_dir.join(format!("0007{suffix}"));
    let new_path = new_dir.join(format!("0008{suffix}"));
    std::fs::write(&old_path, "old content").expect("write old");
    std::thread::sleep(Duration::from_millis(10));
    std::fs::write(&new_path, "new content").expect("write new");

    let stored = read_latest_stored_source(root, "https://example.com/docs")
        .await
        .expect("stored source lookup")
        .expect("stored source exists");
    assert_eq!(stored.content, "new content");
    assert_eq!(stored.path, new_path);
}

#[test]
fn is_stale_marks_old_timestamps_only() {
    let stale_after = Duration::from_secs(60);
    let recent = SystemTime::now() - Duration::from_secs(30);
    let old = SystemTime::now() - Duration::from_secs(300);
    assert!(!is_stale(recent, stale_after));
    assert!(is_stale(old, stale_after));
}
