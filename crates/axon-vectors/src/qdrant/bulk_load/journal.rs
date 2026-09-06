use sha2::{Digest, Sha256};
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::BulkLoadKey;

#[cfg(windows)]
#[allow(unsafe_code)]
fn replace_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    // LEARNED: std::fs::rename cannot atomically replace an existing file on Windows.
    // PATTERN: request replace-existing plus write-through for durable journal swaps.
    use std::os::windows::ffi::OsStrExt as _;
    let source = temporary
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), 0x1 | 0x8) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(temporary, destination)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct PendingBulkLoad {
    pub(super) endpoint: String,
    pub(super) collection: String,
    pub(super) restore_threshold: u64,
}

pub(super) struct BulkLoadJournal {
    pub(super) path: PathBuf,
}

#[cfg(test)]
static FAIL_COMPLETE: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<PathBuf>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JournalWriteBoundary {
    BeforeRename,
    BeforeParentSync,
}

static JOURNAL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl BulkLoadJournal {
    pub(super) fn acquire_collection_lease(&self, key: &BulkLoadKey) -> std::io::Result<File> {
        use fs2::FileExt as _;
        let digest = Sha256::digest(format!("{}\0{}", key.endpoint, key.collection));
        let path = self
            .path
            .with_file_name(format!("qdrant-bulk-{}.lease", hex::encode(digest)));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(path)?;
        file.lock_exclusive()?;
        Ok(file)
    }

    pub(super) fn open(data_dir: &Path) -> std::io::Result<Self> {
        match std::fs::symlink_metadata(data_dir) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(std::io::Error::other(
                    "bulk journal root is not a real directory",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir_all(data_dir)?;
            }
            Err(error) => return Err(error),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(data_dir, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self {
            path: data_dir.join("qdrant-bulk-load-transitions.json"),
        })
    }

    pub(super) fn pending(&self) -> std::io::Result<Vec<PendingBulkLoad>> {
        let _guard = JOURNAL_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.with_process_lock(|| self.read_unlocked())
    }

    pub(super) fn record(&self, key: &BulkLoadKey, restore_threshold: u64) -> std::io::Result<()> {
        let _guard = JOURNAL_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.with_process_lock(|| {
            let mut pending = self.read_unlocked()?;
            pending.retain(|entry| {
                entry.endpoint != key.endpoint || entry.collection != key.collection
            });
            pending.push(PendingBulkLoad {
                endpoint: key.endpoint.clone(),
                collection: key.collection.clone(),
                restore_threshold,
            });
            self.write_unlocked(&pending)
        })
    }

    pub(super) fn complete(&self, key: &BulkLoadKey) -> std::io::Result<()> {
        #[cfg(test)]
        if FAIL_COMPLETE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.path)
        {
            return Err(std::io::Error::other("injected journal completion failure"));
        }
        let _guard = JOURNAL_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.with_process_lock(|| {
            let mut pending = self.read_unlocked()?;
            pending.retain(|entry| {
                entry.endpoint != key.endpoint || entry.collection != key.collection
            });
            self.write_unlocked(&pending)
        })
    }

    #[cfg(test)]
    pub(super) fn inject_next_complete_failure(&self) {
        FAIL_COMPLETE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(self.path.clone());
    }

    fn with_process_lock<T>(
        &self,
        operation: impl FnOnce() -> std::io::Result<T>,
    ) -> std::io::Result<T> {
        use fs2::FileExt as _;
        let lock_path = self.path.with_extension("lock");
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let lock = options.open(lock_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            lock.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        lock.lock_exclusive()?;
        operation()
    }

    fn read_unlocked(&self) -> std::io::Result<Vec<PendingBulkLoad>> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        match options.open(&self.path) {
            Ok(mut file) => {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)?;
                serde_json::from_slice(&bytes)
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    fn write_unlocked(&self, pending: &[PendingBulkLoad]) -> std::io::Result<()> {
        self.write_unlocked_with(pending, |_| Ok(()))
    }

    pub(super) fn write_unlocked_with(
        &self,
        pending: &[PendingBulkLoad],
        mut at_boundary: impl FnMut(JournalWriteBoundary) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        let temporary = self
            .path
            .with_extension(format!("json.{}.tmp", uuid::Uuid::new_v4()));
        let result =
            (|| {
                let bytes = serde_json::to_vec(pending)?;
                let mut file = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&temporary)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt as _;
                    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
                }
                file.write_all(&bytes)?;
                file.sync_all()?;
                at_boundary(JournalWriteBoundary::BeforeRename)?;
                replace_file(&temporary, &self.path)?;
                at_boundary(JournalWriteBoundary::BeforeParentSync)?;
                #[cfg(unix)]
                File::open(self.path.parent().ok_or_else(|| {
                    std::io::Error::other("bulk journal has no parent directory")
                })?)?
                .sync_all()?;
                Ok(())
            })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
}
