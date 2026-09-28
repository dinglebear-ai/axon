//! Bounded acquisition from verified discovery snapshots.
use axon_api::source::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use std::path::Path;

use super::{discovery, local_io, timestamp};
use crate::adapter::Result;
use crate::local_select::LocalOptions;

pub(super) fn acquire_item(
    item: &ManifestItem,
    root: &Path,
    spool_dir: &Path,
    options: &LocalOptions,
    remaining: u64,
) -> Result<(AcquiredSourceItem, u64)> {
    local_io::validate_item_key(&item.source_item_key.0)?;
    let mut item = item.clone();
    let already_omitted = item
        .metadata
        .get(CONTENT_OMISSION_METADATA_KEY)
        .and_then(serde_json::Value::as_str)
        == Some("size_limit_exceeded");
    let payload = if already_omitted
        || item
            .size_bytes
            .is_some_and(|size| size > options.max_file_bytes)
    {
        crate::file_payload::FilePayload {
            bytes: None,
            bytes_read: 0,
        }
    } else {
        let path = root.join(&item.source_item_key.0);
        let file = std::fs::File::open(discovery::spool_path(spool_dir, &item.source_item_key.0))
            .map_err(|error| {
            local_io::fs_error("adapter.local.spool_read_failed", &path, error)
        })?;
        crate::file_payload::read_file(
            file,
            &item.source_item_key.0,
            options.max_file_bytes,
            remaining,
        )?
    };
    let content_ref = match payload.bytes {
        Some(bytes) => {
            let hash = format!("sha256:{:x}", Sha256::digest(&bytes));
            if item.size_bytes != Some(bytes.len() as u64)
                || item.content_hash.as_deref() != Some(&hash)
            {
                let mut error = ApiError::new(
                    "adapter.local.source_changed",
                    ErrorStage::Fetching,
                    "local source changed between discovery and acquisition; retry the source job",
                )
                .with_context("source_item_key", item.source_item_key.0.clone());
                error.retryable = true;
                return Err(error);
            }
            ContentRef::InlineBytes {
                bytes_base64: STANDARD.encode(bytes),
                mime_type: "application/octet-stream".into(),
            }
        }
        None => {
            item.content_hash = None;
            item.metadata.insert(
                CONTENT_OMISSION_METADATA_KEY.into(),
                "size_limit_exceeded".into(),
            );
            ContentRef::InlineBytes {
                bytes_base64: String::new(),
                mime_type: "application/octet-stream".into(),
            }
        }
    };
    Ok((
        AcquiredSourceItem {
            manifest_item: item,
            fetch_status: LifecycleStatus::Completed,
            content_ref,
            raw_artifact_id: None,
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            fetched_at: timestamp(),
            metadata: MetadataMap::new(),
        },
        payload.bytes_read,
    ))
}
