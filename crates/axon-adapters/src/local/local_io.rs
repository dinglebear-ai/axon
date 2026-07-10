use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use axon_api::source::{ApiError, ContentRef};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use sha2::{Digest, Sha256};

use crate::adapter::Result;
use crate::local_select::LocalOptions;

pub(crate) fn read_content_ref(path: &Path, options: &LocalOptions) -> Result<ContentRef> {
    enforce_read_size(path, options)?;
    if options.includes_binary_body(path) {
        let bytes =
            fs::read(path).map_err(|err| fs_error("adapter.local.read_failed", path, err))?;
        return Ok(ContentRef::InlineBytes {
            bytes_base64: BASE64_STANDARD.encode(bytes),
            mime_type: "application/octet-stream".to_string(),
        });
    }
    let text =
        fs::read_to_string(path).map_err(|err| fs_error("adapter.local.read_failed", path, err))?;
    Ok(ContentRef::InlineText { text })
}

pub(crate) fn safe_item_path(root: &Path, item_key: &str) -> Result<PathBuf> {
    let key = Path::new(item_key);
    if key.is_absolute()
        || key
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(ApiError::new(
            "adapter.local.item_key.escape",
            axon_error::ErrorStage::Fetching,
            "local source item key escapes the local source root",
        ));
    }
    let root = fs::canonicalize(root)
        .map_err(|err| fs_error("adapter.local.root_stat_failed", root, err))?;
    let candidate = root.join(key);
    let canonical = fs::canonicalize(&candidate)
        .map_err(|err| fs_error("adapter.local.stat_failed", &candidate, err))?;
    if canonical.starts_with(&root) {
        Ok(canonical)
    } else {
        Err(ApiError::new(
            "adapter.local.item_key.escape",
            axon_error::ErrorStage::Fetching,
            "local source item key escapes the local source root",
        ))
    }
}

pub(crate) fn content_fingerprint(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut file =
        fs::File::open(path).map_err(|err| fs_error("adapter.local.read_failed", path, err))?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| fs_error("adapter.local.read_failed", path, err))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn enforce_read_size(path: &Path, options: &LocalOptions) -> Result<()> {
    let Some(max_file_bytes) = options.max_file_bytes else {
        return Ok(());
    };
    let metadata =
        fs::metadata(path).map_err(|err| fs_error("adapter.local.stat_failed", path, err))?;
    if metadata.len() <= max_file_bytes {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.local.file_too_large",
        axon_error::ErrorStage::Fetching,
        "local source item exceeds max_file_bytes",
    )
    .with_context("path_hint", public_path_hint(path))
    .with_context("max_file_bytes", max_file_bytes.to_string()))
}

pub(crate) fn fs_error(code: &'static str, path: &Path, err: std::io::Error) -> ApiError {
    ApiError::new(code, axon_error::ErrorStage::Discovering, err.to_string())
        .with_context("path_hint", public_path_hint(path))
}

pub(crate) fn public_path_hint(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| "local-source-item".to_string())
}
