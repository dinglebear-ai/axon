use std::fs::{self, File};
use std::io::{Read, Seek};
use std::path::{Component, Path, PathBuf};

use axon_api::source::{ApiError, ContentRef, SourceScope};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use sha2::{Digest, Sha256};

use crate::adapter::Result;
use crate::local_select::LocalOptions;

#[cfg(target_os = "linux")]
use rustix::fs::{Mode, OFlags, ResolveFlags, open, openat2};

#[derive(Debug)]
pub(crate) struct LocalRootHandle {
    #[cfg(target_os = "linux")]
    directory: rustix::fd::OwnedFd,
    #[cfg(not(target_os = "linux"))]
    directory: PathBuf,
}

impl LocalRootHandle {
    #[cfg(target_os = "linux")]
    pub(crate) fn from_allowed_roots(
        source_root: &Path,
        scope: SourceScope,
        allowed_roots: &[PathBuf],
    ) -> Result<Self> {
        let mut matching_roots = allowed_roots
            .iter()
            .filter(|allowed_root| source_root.starts_with(allowed_root))
            .collect::<Vec<_>>();
        matching_roots
            .sort_by_key(|allowed_root| std::cmp::Reverse(allowed_root.components().count()));

        for allowed_root in matching_roots {
            let Ok(relative) = source_root.strip_prefix(allowed_root) else {
                continue;
            };
            let Ok(allowed_fd) = open(
                allowed_root,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW,
                Mode::empty(),
            ) else {
                continue;
            };
            let source_relative = if relative.as_os_str().is_empty() {
                Path::new(".")
            } else {
                relative
            };
            let directory_relative = if scope == SourceScope::File {
                source_relative.parent().unwrap_or_else(|| Path::new("."))
            } else {
                source_relative
            };
            let Ok(directory) = openat2(
                &allowed_fd,
                directory_relative,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW,
                Mode::empty(),
                containment_flags(),
            ) else {
                continue;
            };
            if scope == SourceScope::File {
                let Some(file_name) = source_relative.file_name().and_then(|name| name.to_str())
                else {
                    continue;
                };
                if openat2(
                    &directory,
                    file_name,
                    OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                    Mode::empty(),
                    containment_flags(),
                )
                .is_err()
                {
                    continue;
                }
            }
            return Ok(Self { directory });
        }
        Err(containment_denied(source_root))
    }

    #[cfg(not(target_os = "linux"))]
    pub(crate) fn from_allowed_roots(
        source_root: &Path,
        _scope: SourceScope,
        _allowed_roots: &[PathBuf],
    ) -> Result<Self> {
        Err(ApiError::new(
            "adapter.local.containment_unsupported",
            axon_error::ErrorStage::Authorizing,
            "contained server local sources require Linux openat2",
        )
        .with_context("path_hint", public_path_hint(source_root)))
    }

    pub(crate) fn for_source(root: &Path, scope: SourceScope) -> Result<Self> {
        let metadata = fs::symlink_metadata(root).map_err(|err| root_unsafe(root, err))?;
        if metadata.file_type().is_symlink() {
            return Err(root_unsafe(
                root,
                std::io::Error::other("local source root is a symlink"),
            ));
        }
        if scope == SourceScope::File {
            if !metadata.is_file() {
                return Err(root_unsafe(
                    root,
                    std::io::Error::other("local file source is not a file"),
                ));
            }
            return Self::open(root.parent().unwrap_or_else(|| Path::new(".")));
        }
        Self::open(root)
    }

    pub(crate) fn open(root: &Path) -> Result<Self> {
        reject_unsafe_root(root)?;
        #[cfg(target_os = "linux")]
        {
            let directory = open(
                root,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(|err| root_unsafe(root, err.into()))?;
            Ok(Self { directory })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let directory = fs::canonicalize(root)
                .map_err(|err| fs_error("adapter.local.root_stat_failed", root, err))?;
            Ok(Self { directory })
        }
    }

    pub(crate) fn open_file(&self, item_key: &str) -> Result<File> {
        validate_item_key(item_key)?;
        #[cfg(target_os = "linux")]
        {
            let fd = openat2(
                &self.directory,
                item_key,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::empty(),
                ResolveFlags::BENEATH
                    | ResolveFlags::NO_SYMLINKS
                    | ResolveFlags::NO_MAGICLINKS
                    | ResolveFlags::NO_XDEV,
            )
            .map_err(|_| containment_denied(Path::new(item_key)))?;
            Ok(fd.into())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let path = safe_item_path_from_canonical_root(&self.directory, item_key)?;
            File::open(&path).map_err(|err| fs_error("adapter.local.read_failed", &path, err))
        }
    }
}

#[cfg(target_os = "linux")]
fn containment_flags() -> ResolveFlags {
    ResolveFlags::BENEATH
        | ResolveFlags::NO_SYMLINKS
        | ResolveFlags::NO_MAGICLINKS
        | ResolveFlags::NO_XDEV
}

pub(crate) fn read_content_ref(path: &Path, options: &LocalOptions) -> Result<ContentRef> {
    let file = File::open(path).map_err(|err| fs_error("adapter.local.read_failed", path, err))?;
    read_content_ref_from_file(file, path, options)
}

pub(crate) fn read_content_ref_from_file(
    file: File,
    path_hint: &Path,
    options: &LocalOptions,
) -> Result<ContentRef> {
    enforce_read_size_from_file(&file, path_hint, options)?;
    let bytes = match options.max_file_bytes {
        Some(max_file_bytes) => read_bounded(file, path_hint, max_file_bytes)?,
        None => {
            let mut file = file;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|err| fs_error("adapter.local.read_failed", path_hint, err))?;
            bytes
        }
    };
    if options.includes_binary_body(path_hint) {
        return Ok(ContentRef::InlineBytes {
            bytes_base64: BASE64_STANDARD.encode(bytes),
            mime_type: "application/octet-stream".to_string(),
        });
    }
    let text = String::from_utf8(bytes).map_err(|err| {
        fs_error(
            "adapter.local.read_failed",
            path_hint,
            std::io::Error::new(std::io::ErrorKind::InvalidData, err),
        )
    })?;
    Ok(ContentRef::InlineText { text })
}

fn read_bounded(reader: impl Read, path_hint: &Path, max_file_bytes: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(max_file_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|err| fs_error("adapter.local.read_failed", path_hint, err))?;
    if bytes.len() as u64 > max_file_bytes {
        return Err(ApiError::new(
            "adapter.local.file_too_large",
            axon_error::ErrorStage::Fetching,
            "local source item exceeds max_file_bytes while reading",
        )
        .with_context("path_hint", public_path_hint(path_hint))
        .with_context("max_file_bytes", max_file_bytes.to_string()));
    }
    Ok(bytes)
}

pub(crate) fn safe_item_path(root: &Path, item_key: &str) -> Result<PathBuf> {
    validate_item_key(item_key)?;
    let root = fs::canonicalize(root)
        .map_err(|err| fs_error("adapter.local.root_stat_failed", root, err))?;
    safe_item_path_from_canonical_root(&root, item_key)
}

fn safe_item_path_from_canonical_root(root: &Path, item_key: &str) -> Result<PathBuf> {
    validate_item_key(item_key)?;
    let key = Path::new(item_key);
    let candidate = root.join(key);
    let canonical = fs::canonicalize(&candidate)
        .map_err(|err| fs_error("adapter.local.stat_failed", &candidate, err))?;
    if canonical.starts_with(root) {
        Ok(canonical)
    } else {
        Err(containment_denied(&candidate))
    }
}

pub(crate) fn content_fingerprint(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|err| fs_error("adapter.local.read_failed", path, err))?;
    content_fingerprint_from_file(file, path)
}

pub(crate) fn content_fingerprint_from_file(mut file: File, path_hint: &Path) -> Result<String> {
    file.rewind()
        .map_err(|err| fs_error("adapter.local.read_failed", path_hint, err))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| fs_error("adapter.local.read_failed", path_hint, err))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn enforce_read_size_from_file(
    file: &File,
    path_hint: &Path,
    options: &LocalOptions,
) -> Result<()> {
    let Some(max_file_bytes) = options.max_file_bytes else {
        return Ok(());
    };
    let metadata = file
        .metadata()
        .map_err(|err| fs_error("adapter.local.stat_failed", path_hint, err))?;
    if metadata.len() <= max_file_bytes {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.local.file_too_large",
        axon_error::ErrorStage::Fetching,
        "local source item exceeds max_file_bytes",
    )
    .with_context("path_hint", public_path_hint(path_hint))
    .with_context("max_file_bytes", max_file_bytes.to_string()))
}

fn validate_item_key(item_key: &str) -> Result<()> {
    let key = Path::new(item_key);
    if !key.is_absolute()
        && !key
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.local.item_key.escape",
        axon_error::ErrorStage::Fetching,
        "local source item key escapes the local source root",
    ))
}

fn reject_unsafe_root(root: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(root).map_err(|err| root_unsafe(root, err))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(root_unsafe(
            root,
            std::io::Error::other("local source root is not a real directory"),
        ));
    }
    Ok(())
}

fn root_unsafe(path: &Path, _err: std::io::Error) -> ApiError {
    ApiError::new(
        "adapter.local.root_unsafe",
        axon_error::ErrorStage::Authorizing,
        "local source root is not a safe directory",
    )
    .with_context("path_hint", public_path_hint(path))
}

fn containment_denied(path: &Path) -> ApiError {
    ApiError::new(
        "adapter.local.item_key.escape",
        axon_error::ErrorStage::Fetching,
        "local source containment denied",
    )
    .with_context("path_hint", public_path_hint(path))
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

#[cfg(test)]
#[path = "local_io_tests.rs"]
mod tests;
