use crate::core::content::url_to_filename;
use crate::services::types::{DocumentBackend, PagedDocument};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DocumentCursor {
    backend: DocumentBackend,
    offset: usize,
}

#[derive(Debug, Clone)]
pub struct StoredSourceDocument {
    pub path: PathBuf,
    pub content: String,
    pub modified_at: SystemTime,
}

pub fn paginate_document(
    full_content: &str,
    cursor: Option<&str>,
    token_budget: Option<usize>,
    backend: DocumentBackend,
) -> Result<PagedDocument, Box<dyn Error + Send + Sync>> {
    let offset = match cursor {
        Some(raw) => {
            let decoded = decode_document_cursor(raw)?;
            if decoded.backend != backend {
                return Err(format!(
                    "cursor backend mismatch: expected {backend}, got {}",
                    decoded.backend
                )
                .into());
            }
            decoded.offset
        }
        None => 0,
    };
    let offset_string = offset.to_string();
    let mut page = PagedDocument::from_full_content(
        full_content,
        Some(offset_string.as_str()),
        token_budget,
        backend,
    );
    if let Some(raw_next_cursor) = page.next_cursor.take() {
        let next_offset = raw_next_cursor
            .parse::<usize>()
            .map_err(|err| format!("invalid next cursor offset: {err}"))?;
        page.next_cursor = Some(encode_document_cursor(&DocumentCursor {
            backend,
            offset: next_offset,
        })?);
    }
    Ok(page)
}

pub fn decode_document_cursor_backend(
    cursor: Option<&str>,
) -> Result<Option<DocumentBackend>, Box<dyn Error + Send + Sync>> {
    cursor
        .map(decode_document_cursor)
        .transpose()
        .map(|decoded| decoded.map(|cursor| cursor.backend))
}

pub async fn read_latest_stored_source(
    output_dir: &Path,
    url: &str,
) -> Result<Option<StoredSourceDocument>, Box<dyn Error + Send + Sync>> {
    let output_dir = output_dir.to_path_buf();
    let suffix = document_filename_suffix(url);
    tokio::task::spawn_blocking(move || find_latest_stored_source(&output_dir, &suffix))
        .await
        .map_err(|err| -> Box<dyn Error + Send + Sync> {
            format!("join stored source lookup task: {err}").into()
        })?
}

pub fn is_stale(modified_at: SystemTime, stale_after: Duration) -> bool {
    match SystemTime::now().duration_since(modified_at) {
        Ok(age) => age > stale_after,
        Err(_) => false,
    }
}

fn encode_document_cursor(cursor: &DocumentCursor) -> Result<String, Box<dyn Error + Send + Sync>> {
    let json = serde_json::to_vec(cursor)?;
    Ok(URL_SAFE_NO_PAD.encode(json))
}

fn decode_document_cursor(cursor: &str) -> Result<DocumentCursor, Box<dyn Error + Send + Sync>> {
    let decoded = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|err| format!("invalid document cursor encoding: {err}"))?;
    serde_json::from_slice::<DocumentCursor>(&decoded)
        .map_err(|err| format!("invalid document cursor payload: {err}").into())
}

fn document_filename_suffix(url: &str) -> String {
    let filename = url_to_filename(url, 1);
    filename.chars().skip(4).collect()
}

fn find_latest_stored_source(
    root: &Path,
    suffix: &str,
) -> Result<Option<StoredSourceDocument>, Box<dyn Error + Send + Sync>> {
    if !root.exists() {
        return Ok(None);
    }
    let mut latest: Option<(PathBuf, SystemTime)> = None;
    walk_dir(root, suffix, &mut latest)?;
    let Some((path, modified_at)) = latest else {
        return Ok(None);
    };
    let content = fs::read_to_string(&path)
        .map_err(|err| format!("read stored source {}: {err}", path.display()))?;
    Ok(Some(StoredSourceDocument {
        path,
        content,
        modified_at,
    }))
}

fn walk_dir(
    dir: &Path,
    suffix: &str,
    latest: &mut Option<(PathBuf, SystemTime)>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for entry in fs::read_dir(dir).map_err(|err| format!("read dir {}: {err}", dir.display()))? {
        let entry = entry.map_err(|err| format!("read dir entry {}: {err}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| format!("read file type {}: {err}", path.display()))?;
        if file_type.is_dir() {
            walk_dir(&path, suffix, latest)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if !file_name.ends_with(suffix) {
            continue;
        }
        let modified_at = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .map_err(|err| format!("read metadata {}: {err}", path.display()))?;
        let is_newer = latest
            .as_ref()
            .map(|(_, current)| modified_at > *current)
            .unwrap_or(true);
        if is_newer {
            *latest = Some((path, modified_at));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        decode_document_cursor_backend, is_stale, paginate_document, read_latest_stored_source,
    };
    use crate::services::types::{DocumentBackend, PagedDocument};
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;

    #[test]
    fn paginate_document_obeys_default_budget_boundary() {
        let content =
            "a".repeat(PagedDocument::DEFAULT_TOKEN_BUDGET * PagedDocument::CHARS_PER_TOKEN);
        let page = paginate_document(&content, None, None, DocumentBackend::Qdrant)
            .expect("page document");
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
        let first = paginate_document(&content, None, None, DocumentBackend::StoredSource)
            .expect("first page");
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
    fn decode_document_cursor_backend_returns_expected_backend() {
        let content = "hello world";
        let page = paginate_document(content, None, Some(1), DocumentBackend::LiveScrape)
            .expect("paginated");
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
        let file_name = crate::core::content::url_to_filename("https://example.com/docs", 1);
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
}
