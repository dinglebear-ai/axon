use std::error::Error;
use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

const TEMP_CREATE_ATTEMPTS: usize = 16;

#[derive(Debug)]
pub enum ArtifactWriteError {
    Validation(String),
    RootNotDirectory(PathBuf),
    MissingParent(PathBuf),
    EscapedRoot(PathBuf),
    SymlinkComponent(PathBuf),
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl ArtifactWriteError {
    fn io(operation: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for ArtifactWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) => f.write_str(message),
            Self::RootNotDirectory(path) => {
                write!(f, "artifact root is not a directory: {}", path.display())
            }
            Self::MissingParent(path) => {
                write!(f, "artifact path has no parent: {}", path.display())
            }
            Self::EscapedRoot(path) => {
                write!(f, "artifact path escaped output root: {}", path.display())
            }
            Self::SymlinkComponent(path) => {
                write!(f, "artifact path contains symlink: {}", path.display())
            }
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                f,
                "artifact write failed during {operation} for {}: {source}",
                path.display()
            ),
        }
    }
}

impl Error for ArtifactWriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn reject_unsafe_relative_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("artifact relative_path is empty".to_string());
    }
    if Path::new(path).components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(format!("unsafe artifact relative_path: {path}"));
    }
    Ok(())
}

pub async fn atomic_write_under(
    root: impl AsRef<Path>,
    relative_path: impl AsRef<Path>,
    bytes: &[u8],
) -> Result<PathBuf, ArtifactWriteError> {
    let root = root.as_ref();
    let relative = relative_path.as_ref();
    let relative_string = relative.to_string_lossy().replace('\\', "/");
    reject_unsafe_relative_path(&relative_string).map_err(ArtifactWriteError::Validation)?;

    if root.exists() && !root.is_dir() {
        return Err(ArtifactWriteError::RootNotDirectory(root.to_path_buf()));
    }
    tokio::fs::create_dir_all(root)
        .await
        .map_err(|err| ArtifactWriteError::io("create root directory", root, err))?;
    let canonical_root = tokio::fs::canonicalize(root)
        .await
        .map_err(|err| ArtifactWriteError::io("canonicalize root", root, err))?;

    let final_path = root.join(relative);
    let parent = final_path
        .parent()
        .ok_or_else(|| ArtifactWriteError::MissingParent(final_path.clone()))?;
    create_parent_dirs_under_root(root, relative).await?;
    let canonical_parent = tokio::fs::canonicalize(parent)
        .await
        .map_err(|err| ArtifactWriteError::io("canonicalize parent", parent, err))?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(ArtifactWriteError::EscapedRoot(final_path));
    }

    let file_name = final_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("artifact");
    let (tmp_path, mut file) = create_unique_temp_file(parent, file_name).await?;
    let write_result = async {
        tokio::io::AsyncWriteExt::write_all(&mut file, bytes)
            .await
            .map_err(|err| ArtifactWriteError::io("write temp file", &tmp_path, err))?;
        file.sync_all()
            .await
            .map_err(|err| ArtifactWriteError::io("sync temp file", &tmp_path, err))?;
        drop(file);
        atomic_replace_file(&tmp_path, &final_path).await?;
        sync_parent_directory(parent).await?;
        Ok::<(), ArtifactWriteError>(())
    }
    .await;

    if let Err(err) = write_result {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(err);
    }

    Ok(final_path)
}

pub async fn atomic_write_explicit(
    path: impl AsRef<Path>,
    bytes: &[u8],
) -> Result<PathBuf, ArtifactWriteError> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .ok_or_else(|| ArtifactWriteError::MissingParent(path.to_path_buf()))?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|err| ArtifactWriteError::io("create parent directory", parent, err))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("artifact");
    let (tmp_path, mut file) = create_unique_temp_file(parent, file_name).await?;
    let write_result = async {
        tokio::io::AsyncWriteExt::write_all(&mut file, bytes)
            .await
            .map_err(|err| ArtifactWriteError::io("write temp file", &tmp_path, err))?;
        file.sync_all()
            .await
            .map_err(|err| ArtifactWriteError::io("sync temp file", &tmp_path, err))?;
        drop(file);
        atomic_replace_file(&tmp_path, path).await?;
        sync_parent_directory(parent).await?;
        Ok::<(), ArtifactWriteError>(())
    }
    .await;

    if let Err(err) = write_result {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(err);
    }

    Ok(path.to_path_buf())
}

async fn create_unique_temp_file(
    parent: &Path,
    file_name: &str,
) -> Result<(PathBuf, tokio::fs::File), ArtifactWriteError> {
    for _ in 0..TEMP_CREATE_ATTEMPTS {
        let path = parent.join(format!(
            ".{file_name}.tmp-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(ArtifactWriteError::io("create temp file", path, error));
            }
        }
    }
    Err(ArtifactWriteError::io(
        "create temp file",
        parent,
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique temporary file",
        ),
    ))
}

/// Atomically replace `destination` with a fully-written file from the same directory.
pub async fn atomic_replace_file(
    temporary: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> Result<(), ArtifactWriteError> {
    let temporary = temporary.as_ref();
    let destination = destination.as_ref();
    atomic_replace_file_platform(temporary, destination)
        .await
        .map_err(|error| ArtifactWriteError::io("replace destination file", destination, error))
}

#[cfg(not(windows))]
async fn atomic_replace_file_platform(temporary: &Path, destination: &Path) -> io::Result<()> {
    tokio::fs::rename(temporary, destination).await
}

#[cfg(windows)]
#[allow(unsafe_code)]
async fn atomic_replace_file_platform(temporary: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    let existing = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replacement = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        MoveFileExW(
            existing.as_ptr(),
            replacement.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
async fn sync_parent_directory(parent: &Path) -> Result<(), ArtifactWriteError> {
    let parent_dir = tokio::fs::File::open(parent)
        .await
        .map_err(|err| ArtifactWriteError::io("open parent directory", parent, err))?;
    parent_dir
        .sync_all()
        .await
        .map_err(|err| ArtifactWriteError::io("sync parent directory", parent, err))
}

#[cfg(windows)]
async fn sync_parent_directory(_parent: &Path) -> Result<(), ArtifactWriteError> {
    // MoveFileExW with MOVEFILE_WRITE_THROUGH flushes the replacement operation.
    Ok(())
}

pub async fn write_configured_output(
    output_dir: impl AsRef<Path>,
    output_path: Option<&Path>,
    default_relative_path: impl AsRef<Path>,
    bytes: &[u8],
) -> Result<PathBuf, ArtifactWriteError> {
    match output_path {
        Some(path) => atomic_write_explicit(path, bytes).await,
        None => atomic_write_under(output_dir, default_relative_path, bytes).await,
    }
}

pub async fn write_managed_output(
    output_dir: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    bytes: &[u8],
) -> Result<PathBuf, ArtifactWriteError> {
    let output_dir = output_dir.as_ref();
    let output_path = output_path.as_ref();
    let relative_path = output_path
        .strip_prefix(output_dir)
        .map_err(|_| ArtifactWriteError::EscapedRoot(output_path.to_path_buf()))?;
    atomic_write_under(output_dir, relative_path, bytes).await
}

async fn create_parent_dirs_under_root(
    root: &Path,
    relative_path: &Path,
) -> Result<(), ArtifactWriteError> {
    let Some(parent_relative) = relative_path.parent() else {
        return Ok(());
    };
    let mut current = root.to_path_buf();
    for component in parent_relative.components() {
        let Component::Normal(segment) = component else {
            return Err(ArtifactWriteError::Validation(format!(
                "unsafe artifact relative_path: {}",
                relative_path.display()
            )));
        };
        current.push(segment);
        match tokio::fs::symlink_metadata(&current).await {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(ArtifactWriteError::SymlinkComponent(current));
                }
                if !meta.is_dir() {
                    return Err(ArtifactWriteError::RootNotDirectory(current));
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                tokio::fs::create_dir(&current).await.map_err(|err| {
                    ArtifactWriteError::io("create parent directory", &current, err)
                })?;
            }
            Err(err) => {
                return Err(ArtifactWriteError::io(
                    "read parent directory metadata",
                    &current,
                    err,
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;
