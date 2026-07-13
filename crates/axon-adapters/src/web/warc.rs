//! Minimal WARC 1.1 `response`-record writer for the web adapter's per-item
//! acquire loop (issue #298 Wave 2b regression 2 — `--warc <PATH>` /
//! `validated_options.warc_path`).
//!
//! The relocated crawl engine (`web_engine::engine::runtime`) already writes
//! WARC output for a *whole crawl* via spider's own `Website::with_warc` +
//! `spider::utils::warc::WarcWriter`, which drains a broadcast channel of
//! spider's internal `Page` type. That type's fields (`url`, `html`, ...) are
//! `pub(crate)` to the `spider` crate with no public constructor, so a `Page`
//! cannot be built here from an [`AcquiredSourceItem`] — and
//! `providers::http_fetch::HttpFetchProvider`'s raw-reqwest path never
//! produces a spider `Page` at all. This module is therefore a small,
//! independent WARC/1.1 writer operating directly on `AcquiredSourceItem`, so
//! **both** HTTP-fetched and Chrome-rendered items land in the same archive
//! regardless of which provider produced them — mirroring the record shape
//! spider's own (private) `serialize_page` produces (`WARC/1.1`,
//! `WARC-Type: response`, a synthesized `HTTP/1.1 <status> ...` payload).
//!
//! **Fallback note (design intent, not silently dropped):** the pipeline's
//! artifact side effects are meant to flow through `ArtifactStore::put` so a
//! WARC archive becomes a tracked `ArtifactRef` the same way other stage
//! artifacts are recorded. `WebSourceAdapter` is not currently constructed
//! with an `Arc<dyn ArtifactStore>` (see `crates/axon-services/src/web_source.rs`
//! and `.../web_source/vectorize.rs`), and wiring that in — plus updating both
//! construction sites — is a larger, separate change than this regression-fix
//! slice. This module writes directly to the configured file path instead; the
//! resulting file is still surfaced as an `ArtifactKind::Warc` `ArtifactRef` in
//! `SourceAcquisition.artifacts` (built in `acquire.rs`) so it isn't invisible
//! to callers, but it was written by a direct file handle, not `ArtifactStore::put`.
//! Follow-up: thread `Arc<dyn ArtifactStore>` into `WebSourceAdapter::new` and
//! replace `warc::open`/`warc::append_item` with an `ArtifactStore::put` call
//! once the whole item set for a generation is known.

use std::io;
use std::io::Write as _;
use std::path::Path;

use axon_api::source::*;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use super::metadata::mime_type_for_content_kind;

const CRLF: &[u8] = b"\r\n";

/// Open (truncate-create) the WARC file at `path` and write the leading
/// `warcinfo` record — one archive per acquisition run, mirroring
/// `spider::utils::warc::WarcWriter::create`'s per-run semantics.
pub(super) async fn open(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = File::create(path).await?;
    file.write_all(&warcinfo_record()).await?;
    Ok(file)
}

/// Append one `response` record built from an already-acquired item.
pub(super) async fn append_item(file: &mut File, item: &AcquiredSourceItem) -> io::Result<()> {
    file.write_all(&response_record(item)).await
}

fn warcinfo_record() -> Vec<u8> {
    let payload = format!(
        "software: axon/{}\r\nformat: WARC File Format 1.1\r\n",
        env!("CARGO_PKG_VERSION")
    );
    let mut buf = Vec::with_capacity(256 + payload.len());
    write_record_headers(
        &mut buf,
        "warcinfo",
        None,
        "application/warc-fields",
        payload.len(),
    );
    buf.extend_from_slice(payload.as_bytes());
    buf.extend_from_slice(CRLF);
    buf.extend_from_slice(CRLF);
    buf
}

fn response_record(item: &AcquiredSourceItem) -> Vec<u8> {
    let uri = &item.manifest_item.canonical_uri;
    let status = item
        .metadata
        .get("web_status")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(200);
    let content_kind = item
        .manifest_item
        .content_kind
        .unwrap_or(ContentKind::Markdown);
    let body = item_body_bytes(&item.content_ref);

    let mut payload = Vec::with_capacity(256 + body.len());
    let _ = write!(payload, "HTTP/1.1 {status} {}\r\n", reason_phrase(status));
    let _ = write!(
        payload,
        "Content-Type: {}\r\n",
        mime_type_for_content_kind(content_kind)
    );
    let _ = write!(payload, "Content-Length: {}\r\n", body.len());
    payload.extend_from_slice(CRLF);
    payload.extend_from_slice(&body);

    let mut buf = Vec::with_capacity(256 + payload.len());
    write_record_headers(
        &mut buf,
        "response",
        Some(uri),
        "application/http; msgtype=response",
        payload.len(),
    );
    buf.extend_from_slice(&payload);
    buf.extend_from_slice(CRLF);
    buf.extend_from_slice(CRLF);
    buf
}

fn write_record_headers(
    buf: &mut Vec<u8>,
    record_type: &str,
    target_uri: Option<&str>,
    content_type: &str,
    content_length: usize,
) {
    let _ = write!(buf, "WARC/1.1\r\n");
    let _ = write!(buf, "WARC-Type: {record_type}\r\n");
    let _ = write!(
        buf,
        "WARC-Record-ID: <urn:uuid:{}>\r\n",
        uuid::Uuid::new_v4()
    );
    let _ = write!(
        buf,
        "WARC-Date: {}\r\n",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    );
    if let Some(uri) = target_uri {
        let _ = write!(buf, "WARC-Target-URI: {uri}\r\n");
    }
    let _ = write!(buf, "Content-Type: {content_type}\r\n");
    let _ = write!(buf, "Content-Length: {content_length}\r\n");
    buf.extend_from_slice(CRLF);
}

fn item_body_bytes(content_ref: &ContentRef) -> Vec<u8> {
    match content_ref {
        ContentRef::InlineText { text } => text.clone().into_bytes(),
        ContentRef::InlineBytes { bytes_base64, .. } => {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .decode(bytes_base64)
                .unwrap_or_default()
        }
        // Neither provider produces these today (see `acquire.rs`); an empty
        // body keeps the WARC record well-formed rather than failing archival.
        ContentRef::Artifact { .. } | ContentRef::External { .. } => Vec::new(),
    }
}

fn reason_phrase(status: u64) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        404 => "Not Found",
        _ => "Unknown",
    }
}

/// Build the `ArtifactRef` surfaced in `SourceAcquisition.artifacts` for the
/// WARC file written by this acquisition run. See the module doc's
/// "Fallback note" — this describes a file written directly to disk, not one
/// put through `ArtifactStore`.
pub(super) async fn artifact_ref(path: &Path) -> ArtifactRef {
    let size_bytes = tokio::fs::metadata(path).await.ok().map(|m| m.len());
    ArtifactRef {
        artifact_id: ArtifactId::new(format!("warc:{}", path.display())),
        artifact_kind: ArtifactKind::Warc,
        uri: path.display().to_string(),
        size_bytes,
        content_hash: None,
        created_at: super::timestamp(),
    }
}

#[cfg(test)]
#[path = "warc_tests.rs"]
mod tests;
