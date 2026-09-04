use super::*;

#[cfg(not(unix))]
mod replace;
#[cfg(not(unix))]
use replace::replace_file;
mod names;
#[cfg(unix)]
use names::component_name;
use names::lease_name;
#[cfg(unix)]
mod temp;
#[cfg(unix)]
use temp::TempFile;

#[cfg(unix)]
#[derive(Debug)]
pub(super) struct SecureJournalDir {
    root: PathBuf,
    directory: std::fs::File,
}

#[cfg(unix)]
#[allow(unsafe_code)]
impl SecureJournalDir {
    pub(super) fn open(root: &Path) -> anyhow::Result<Self> {
        use std::os::unix::ffi::OsStrExt as _;
        use std::os::unix::fs::MetadataExt as _;
        use std::os::unix::io::FromRawFd as _;

        if let Err(error) = std::fs::create_dir(root)
            && error.kind() != std::io::ErrorKind::AlreadyExists
        {
            return Err(error.into());
        }
        let expected = std::fs::symlink_metadata(root)?;
        if expected.file_type().is_symlink() || !expected.is_dir() {
            anyhow::bail!("artifact cleanup journal root is not a real directory");
        }
        let path = std::ffi::CString::new(root.as_os_str().as_bytes())?;
        let fd = unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            anyhow::bail!(
                "artifact cleanup journal root is not a real directory: {}",
                std::io::Error::last_os_error()
            );
        }
        let directory = unsafe { std::fs::File::from_raw_fd(fd) };
        let actual = directory.metadata()?;
        if actual.dev() != expected.dev() || actual.ino() != expected.ino() {
            anyhow::bail!("artifact cleanup journal root changed while opening");
        }
        if unsafe { libc::fchmod(std::os::fd::AsRawFd::as_raw_fd(&directory), 0o700) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let opened = Self {
            root: root.to_path_buf(),
            directory,
        };
        #[cfg(test)]
        if let Some((displaced, external)) = ROOT_SWAPS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(root)
        {
            std::fs::rename(root, displaced)?;
            std::os::unix::fs::symlink(external, root)?;
        }
        Ok(opened)
    }

    pub(super) fn verify_path(&self) -> anyhow::Result<()> {
        use std::os::unix::fs::MetadataExt as _;
        let held = self.directory.metadata()?;
        let current = std::fs::symlink_metadata(&self.root)?;
        if current.file_type().is_symlink()
            || !current.is_dir()
            || current.dev() != held.dev()
            || current.ino() != held.ino()
        {
            anyhow::bail!("artifact cleanup journal root changed while in use");
        }
        Ok(())
    }

    pub(super) fn rewrite(
        &self,
        token: &JournalToken,
        record: &ArtifactCleanupJournalRecord,
    ) -> anyhow::Result<()> {
        use std::io::Write as _;
        use std::os::fd::{AsRawFd as _, FromRawFd as _};

        self.verify_path()?;
        let destination = component_name(&token.0)?;
        let temporary = format!(".journal-{}.tmp", uuid::Uuid::new_v4());
        #[cfg(test)]
        fail_if_injected(&token.0, JournalFault::Create)?;
        let temp_name = std::ffi::CString::new(temporary.as_str())?;
        let fd = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                temp_name.as_ptr(),
                libc::O_WRONLY | libc::O_CLOEXEC | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut cleanup = TempFile::new(self.directory.as_raw_fd(), temp_name);
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        file.write_all(&serde_json::to_vec(record)?)?;
        #[cfg(test)]
        fail_if_injected(&token.0, JournalFault::FileSync)?;
        file.sync_all()?;
        drop(file);
        #[cfg(test)]
        fail_if_injected(&token.0, JournalFault::Rename)?;
        let destination = std::ffi::CString::new(destination)?;
        if unsafe {
            libc::renameat(
                self.directory.as_raw_fd(),
                cleanup.name.as_ptr(),
                self.directory.as_raw_fd(),
                destination.as_ptr(),
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        cleanup.published = true;
        self.directory.sync_all()?;
        self.verify_path()
    }

    pub(super) fn sweep_stale_temporaries(&self) -> anyhow::Result<()> {
        use std::os::fd::AsRawFd as _;
        self.verify_path()?;
        let cutoff = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(600))
            .unwrap_or(std::time::UNIX_EPOCH);
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let stale = entry
                .metadata()?
                .modified()
                .map(|modified| modified <= cutoff)
                .unwrap_or(false);
            if stale
                && name.ends_with(".tmp")
                && (name.starts_with(".journal-") || name.contains(".owner-"))
            {
                let name = std::ffi::CString::new(name)?;
                if unsafe { libc::unlinkat(self.directory.as_raw_fd(), name.as_ptr(), 0) } != 0 {
                    let error = std::io::Error::last_os_error();
                    if error.kind() != std::io::ErrorKind::NotFound {
                        return Err(error.into());
                    }
                }
            }
        }
        self.directory.sync_all()?;
        Ok(())
    }

    pub(super) fn remove(&self, token: &JournalToken) -> anyhow::Result<()> {
        use std::os::fd::AsRawFd as _;
        self.verify_path()?;
        let owner = token.0.with_extension("owner");
        for path in [&token.0, &owner] {
            let name = std::ffi::CString::new(component_name(path)?)?;
            if unsafe { libc::unlinkat(self.directory.as_raw_fd(), name.as_ptr(), 0) } != 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() != std::io::ErrorKind::NotFound {
                    return Err(error.into());
                }
            }
        }
        self.directory.sync_all()?;
        Ok(())
    }

    pub(super) fn acquire_lease(
        &self,
        pending: &Path,
        claimed: &Path,
        needs_rename: bool,
    ) -> anyhow::Result<Option<std::fs::File>> {
        use fs2::FileExt as _;
        use std::os::fd::{AsRawFd as _, FromRawFd as _};
        self.verify_path()?;
        let lease_name = std::ffi::CString::new(lease_name(claimed)?)?;
        let lease_fd = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                lease_name.as_ptr(),
                libc::O_RDWR | libc::O_CLOEXEC | libc::O_CREAT | libc::O_NOFOLLOW,
                0o600,
            )
        };
        if lease_fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let lease = unsafe { std::fs::File::from_raw_fd(lease_fd) };
        if let Err(error) = lease.try_lock_exclusive() {
            if error.kind() == std::io::ErrorKind::WouldBlock {
                return Ok(None);
            }
            return Err(error.into());
        }
        let claimed_name = std::ffi::CString::new(component_name(claimed)?)?;
        let claimed_exists = unsafe {
            libc::faccessat(
                self.directory.as_raw_fd(),
                claimed_name.as_ptr(),
                libc::F_OK,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } == 0;
        if needs_rename && claimed_exists {
            self.unlink_if_present(pending)?;
        } else if needs_rename {
            let pending_name = std::ffi::CString::new(component_name(pending)?)?;
            if unsafe {
                libc::renameat(
                    self.directory.as_raw_fd(),
                    pending_name.as_ptr(),
                    self.directory.as_raw_fd(),
                    claimed_name.as_ptr(),
                )
            } != 0
            {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::NotFound {
                    return Ok(None);
                }
                return Err(error.into());
            }
        }
        self.directory.sync_all()?;
        self.verify_path()?;
        Ok(Some(lease))
    }

    pub(super) fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        use std::io::Read as _;
        use std::os::fd::{AsRawFd as _, FromRawFd as _};
        self.verify_path()?;
        #[cfg(test)]
        fail_if_injected(path, JournalFault::Read)?;
        let name = std::ffi::CString::new(component_name(path)?)?;
        let fd = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    pub(super) fn quarantine(&self, source: &Path, destination: &Path) -> anyhow::Result<()> {
        use std::os::fd::AsRawFd as _;
        let source = std::ffi::CString::new(component_name(source)?)?;
        let destination = std::ffi::CString::new(component_name(destination)?)?;
        if unsafe {
            libc::renameat(
                self.directory.as_raw_fd(),
                source.as_ptr(),
                self.directory.as_raw_fd(),
                destination.as_ptr(),
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        self.directory.sync_all()?;
        self.verify_path()
    }

    pub(super) fn write_owner(&self, claimed: &Path, bytes: &[u8]) -> anyhow::Result<()> {
        use std::io::Write as _;
        use std::os::fd::{AsRawFd as _, FromRawFd as _};
        let owner = std::ffi::CString::new(component_name(&claimed.with_extension("owner"))?)?;
        let temporary = std::ffi::CString::new(format!(".owner-{}.tmp", uuid::Uuid::new_v4()))?;
        let fd = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                temporary.as_ptr(),
                libc::O_WRONLY | libc::O_CLOEXEC | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut cleanup = TempFile::new(self.directory.as_raw_fd(), temporary);
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        #[cfg(test)]
        fail_if_injected(claimed, JournalFault::OwnerWrite)?;
        file.write_all(bytes)?;
        #[cfg(test)]
        fail_if_injected(claimed, JournalFault::OwnerSync)?;
        file.sync_all()?;
        drop(file);
        #[cfg(test)]
        fail_if_injected(claimed, JournalFault::OwnerRename)?;
        if unsafe {
            libc::renameat(
                self.directory.as_raw_fd(),
                cleanup.name.as_ptr(),
                self.directory.as_raw_fd(),
                owner.as_ptr(),
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        cleanup.published = true;
        self.directory.sync_all()?;
        self.verify_path()
    }

    fn unlink_if_present(&self, path: &Path) -> anyhow::Result<()> {
        use std::os::fd::AsRawFd as _;
        let name = std::ffi::CString::new(component_name(path)?)?;
        if unsafe { libc::unlinkat(self.directory.as_raw_fd(), name.as_ptr(), 0) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(error.into());
            }
        }
        Ok(())
    }
}

#[cfg(not(unix))]
#[derive(Debug)]
pub(super) struct SecureJournalDir {
    root: PathBuf,
}

#[cfg(not(unix))]
impl SecureJournalDir {
    pub(super) fn open(root: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(root)?;
        let metadata = std::fs::symlink_metadata(root)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            anyhow::bail!("artifact cleanup journal root is not a real directory");
        }
        Ok(Self { root: root.into() })
    }
    pub(super) fn verify_path(&self) -> anyhow::Result<()> {
        Ok(())
    }
    pub(super) fn sweep_stale_temporaries(&self) -> anyhow::Result<()> {
        let cutoff = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(600))
            .unwrap_or(std::time::UNIX_EPOCH);
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            let stale = entry
                .metadata()?
                .modified()
                .map(|value| value <= cutoff)
                .unwrap_or(false);
            let path = entry.path();
            if path
                .file_name()
                .and_then(|v| v.to_str())
                .is_some_and(|name| {
                    name.ends_with(".tmp")
                        && (name.starts_with(".journal-") || name.contains(".owner-"))
                })
                && stale
            {
                std::fs::remove_file(path)?;
            }
        }
        Ok(())
    }
    pub(super) fn rewrite(
        &self,
        token: &JournalToken,
        record: &ArtifactCleanupJournalRecord,
    ) -> anyhow::Result<()> {
        let temporary = self
            .root
            .join(format!(".journal-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            std::fs::write(&temporary, serde_json::to_vec(record)?)?;
            replace_file(&temporary, &token.0)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
    pub(super) fn remove(&self, token: &JournalToken) -> anyhow::Result<()> {
        let owner = token.0.with_extension("owner");
        for path in [&token.0, &owner] {
            if let Err(error) = std::fs::remove_file(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(error.into());
            }
        }
        Ok(())
    }
    pub(super) fn acquire_lease(
        &self,
        pending: &Path,
        claimed: &Path,
        needs_rename: bool,
    ) -> anyhow::Result<Option<std::fs::File>> {
        use fs2::FileExt as _;
        let lease = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(self.root.join(lease_name(claimed)?))?;
        if let Err(error) = lease.try_lock_exclusive() {
            if error.kind() == std::io::ErrorKind::WouldBlock {
                return Ok(None);
            }
            return Err(error.into());
        }
        if needs_rename && claimed.exists() {
            let _ = std::fs::remove_file(pending);
        } else if needs_rename {
            std::fs::rename(pending, claimed)?;
        }
        Ok(Some(lease))
    }
    pub(super) fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        Ok(std::fs::read(path)?)
    }
    pub(super) fn quarantine(&self, source: &Path, destination: &Path) -> anyhow::Result<()> {
        std::fs::rename(source, destination)?;
        Ok(())
    }
    pub(super) fn write_owner(&self, claimed: &Path, bytes: &[u8]) -> anyhow::Result<()> {
        let owner = claimed.with_extension("owner");
        let temporary = claimed.with_extension(format!("owner-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            #[cfg(test)]
            fail_if_injected(claimed, JournalFault::OwnerWrite)?;
            std::fs::write(&temporary, bytes)?;
            #[cfg(test)]
            fail_if_injected(claimed, JournalFault::OwnerSync)?;
            std::fs::OpenOptions::new()
                .read(true)
                .open(&temporary)?
                .sync_all()?;
            #[cfg(test)]
            fail_if_injected(claimed, JournalFault::OwnerRename)?;
            replace_file(&temporary, &owner)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
}
