//! Minimal WARC 1.1 archive builder for the web adapter's acquired items
//! (issue #298 Wave 2b regression 2 — `--warc <PATH>` /
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
//! artifact side effects flow through `ArtifactStore::put` in the services
//! layer. This adapter module therefore produces bytes, size, and digest only;
//! it never opens or writes a destination path itself.

use std::io::Write as _;

use axon_api::source::*;
use sha2::{Digest as _, Sha256};

use super::metadata::mime_type_for_content_kind;

const CRLF: &[u8] = b"\r\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarcArchive {
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub size_bytes: u64,
}

pub fn build_archive(items: &[AcquiredSourceItem]) -> WarcArchive {
    let mut bytes = warcinfo_record();
    for item in items {
        bytes.extend_from_slice(&response_record(item));
    }
    let digest = sha256_hex(&bytes);
    let size_bytes = bytes.len() as u64;
    WarcArchive {
        bytes,
        sha256: format!("sha256:{digest}"),
        size_bytes,
    }
}

pub(super) struct StreamingArchive {
    file: tempfile::NamedTempFile,
    digest: Sha256,
    bytes: u64,
}

impl StreamingArchive {
    pub(super) fn new() -> std::io::Result<Self> {
        let mut writer = Self {
            file: tempfile::NamedTempFile::new()?,
            digest: Sha256::new(),
            bytes: 0,
        };
        writer.write_bytes(&warcinfo_record())?;
        Ok(writer)
    }

    pub(super) fn new_in(directory: &std::path::Path) -> std::io::Result<Self> {
        let mut writer = Self {
            file: tempfile::NamedTempFile::new_in(directory)?,
            digest: Sha256::new(),
            bytes: 0,
        };
        writer.write_bytes(&warcinfo_record())?;
        Ok(writer)
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
        let next = self
            .bytes
            .checked_add(bytes.len() as u64)
            .filter(|next| *next <= MAX_ARCHIVE_BYTES)
            .ok_or_else(|| std::io::Error::other("archive exceeds 256 MiB disk admission"))?;
        self.file.write_all(bytes)?;
        self.digest.update(bytes);
        self.bytes = next;
        Ok(())
    }
}

impl crate::SourceArchiveWriter for StreamingArchive {
    fn append(&mut self, item: &AcquiredSourceItem) -> std::io::Result<()> {
        use std::borrow::Cow;
        let body: Cow<'_, [u8]> = match &item.content_ref {
            ContentRef::InlineText { text } => Cow::Borrowed(text.as_bytes()),
            ContentRef::InlineBytes { bytes_base64, .. } => {
                use base64::Engine as _;
                Cow::Owned(
                    base64::engine::general_purpose::STANDARD
                        .decode(bytes_base64)
                        .unwrap_or_default(),
                )
            }
            ContentRef::Artifact { .. } | ContentRef::External { .. } => Cow::Borrowed(&[]),
        };
        let status = item
            .metadata
            .get("web_status")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(200);
        let kind = item
            .manifest_item
            .content_kind
            .unwrap_or(ContentKind::Markdown);
        let payload_header = format!(
            "HTTP/1.1 {status} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
            reason_phrase(status),
            mime_type_for_content_kind(kind),
            body.len()
        );
        let mut header = Vec::new();
        if item.manifest_item.canonical_uri.len() > 64 * 1024 {
            return Err(std::io::Error::other(
                "archive target URI exceeds header admission",
            ));
        }
        write_record_headers(
            &mut header,
            "response",
            Some(&item.manifest_item.canonical_uri),
            "application/http; msgtype=response",
            payload_header.len() + body.len(),
        );
        self.write_bytes(&header)?;
        self.write_bytes(payload_header.as_bytes())?;
        self.write_bytes(&body)?;
        self.write_bytes(b"\r\n\r\n")
    }

    fn finish(self: Box<Self>) -> std::io::Result<crate::GeneratedArchiveFile> {
        self.file.as_file().sync_all()?;
        let mut metadata = MetadataMap::new();
        metadata.insert("artifact_role".into(), serde_json::json!("source_archive"));
        metadata.insert("archive_format".into(), serde_json::json!("warc-1.1"));
        Ok(crate::GeneratedArchiveFile {
            file: self.file,
            kind: ArtifactKind::Warc,
            content_type: "application/warc".into(),
            size_bytes: self.bytes,
            content_hash: format!("sha256:{}", hex::encode(self.digest.finalize())),
            metadata,
        })
    }
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

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
#[path = "warc_tests.rs"]
mod tests;
