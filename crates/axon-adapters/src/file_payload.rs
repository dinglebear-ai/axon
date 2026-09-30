//! Finite filesystem payload reads shared by Git and Local acquisition.
use crate::acquisition::{
    ACQUISITION_BATCH_BYTES_KEY, DEFAULT_ACQUISITION_BATCH_BYTES, MAX_FILE_CONTENT_BYTES,
};
use crate::adapter::Result;
use axon_api::source::*;
use std::fs::File;
use std::io::Read;

pub(crate) struct FilePayload {
    pub bytes: Option<Vec<u8>>,
    pub bytes_read: u64,
}

pub(crate) fn effective_item_limit(plan: &SourcePlan, fallback: u64) -> u64 {
    plan.limits
        .effective
        .max_bytes_per_item
        .unwrap_or(fallback)
        .min(fallback)
        .min(MAX_FILE_CONTENT_BYTES)
}

pub(crate) fn batch_byte_limit(plan: &SourcePlan) -> u64 {
    plan.route
        .source
        .metadata
        .get(ACQUISITION_BATCH_BYTES_KEY)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(DEFAULT_ACQUISITION_BATCH_BYTES)
        .min(DEFAULT_ACQUISITION_BATCH_BYTES)
}

fn io_error(key: &str, error: std::io::Error) -> ApiError {
    ApiError::new(
        "source.acquire.read_failed",
        ErrorStage::Fetching,
        "failed to read source item",
    )
    .with_context("source_item_key", key.to_owned())
    .with_context("io_kind", format!("{:?}", error.kind()))
}

pub(crate) fn read_file(
    file: File,
    key: &str,
    item_limit: u64,
    remaining: u64,
) -> Result<FilePayload> {
    let size = file.metadata().map_err(|e| io_error(key, e))?.len();
    read_file_after_stat(file, key, item_limit, remaining, size)
}

fn read_file_after_stat(
    mut file: File,
    key: &str,
    item_limit: u64,
    remaining: u64,
    size: u64,
) -> Result<FilePayload> {
    let item_limit = item_limit.min(MAX_FILE_CONTENT_BYTES);
    if size > item_limit {
        return Ok(FilePayload {
            bytes: None,
            bytes_read: 0,
        });
    }
    let budget_error = || {
        ApiError::new(
            "source.acquire.byte_budget_exceeded",
            ErrorStage::Fetching,
            "source acquisition byte budget exhausted",
        )
        .with_context("source_item_key", key.to_owned())
    };
    if size > remaining {
        return Err(budget_error());
    }
    let read_limit = item_limit.saturating_add(1).min(remaining);
    let mut bytes = Vec::new();
    (&mut file)
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error(key, e))?;
    let bytes_read = bytes.len() as u64;
    if bytes_read > item_limit {
        return Ok(FilePayload {
            bytes: None,
            bytes_read,
        });
    }
    // Stat the same open file rather than consuming an uncharged EOF probe.
    // Per-item overflow remains an omission even when it coincides with the
    // remaining allowance; all bytes already read are still charged.
    if bytes_read == read_limit {
        let current_size = file.metadata().map_err(|e| io_error(key, e))?.len();
        if current_size > item_limit {
            return Ok(FilePayload {
                bytes: None,
                bytes_read,
            });
        }
        if current_size > bytes_read {
            return Err(budget_error());
        }
    }
    Ok(FilePayload {
        bytes: Some(bytes),
        bytes_read,
    })
}

#[cfg(test)]
#[path = "file_payload_tests.rs"]
mod tests;
