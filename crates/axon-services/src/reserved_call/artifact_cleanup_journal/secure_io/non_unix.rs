use super::*;

#[derive(Debug)]
pub(in crate::reserved_call::artifact_cleanup_journal) struct SecureJournalDir {
    root: PathBuf,
    canonical_root: PathBuf,
    created_at: std::time::SystemTime,
    #[cfg(windows)]
    identity: (Option<u32>, Option<u64>),
}

impl SecureJournalDir {
    pub(in crate::reserved_call::artifact_cleanup_journal) fn open(
        root: &Path,
    ) -> anyhow::Result<Self> {
        #[cfg(windows)]
        use std::os::windows::fs::MetadataExt as _;
        std::fs::create_dir_all(root)?;
        let metadata = std::fs::symlink_metadata(root)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            anyhow::bail!("artifact cleanup journal root is not a real directory");
        }
        Ok(Self {
            root: root.into(),
            canonical_root: std::fs::canonicalize(root)?,
            created_at: metadata.created()?,
            #[cfg(windows)]
            identity: (metadata.volume_serial_number(), metadata.file_index()),
        })
    }
    pub(in crate::reserved_call::artifact_cleanup_journal) fn verify_path(
        &self,
    ) -> anyhow::Result<()> {
        #[cfg(windows)]
        use std::os::windows::fs::MetadataExt as _;
        let metadata = std::fs::symlink_metadata(&self.root)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_dir()
            || std::fs::canonicalize(&self.root)? != self.canonical_root
            || metadata.created()? != self.created_at
            || cfg!(windows) && {
                #[cfg(windows)]
                {
                    (metadata.volume_serial_number(), metadata.file_index()) != self.identity
                }
                #[cfg(not(windows))]
                {
                    false
                }
            }
        {
            anyhow::bail!("artifact cleanup journal root changed while in use");
        }
        Ok(())
    }
    pub(in crate::reserved_call::artifact_cleanup_journal) fn sweep_stale_temporaries(
        &self,
    ) -> anyhow::Result<()> {
        self.verify_path()?;
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
    pub(in crate::reserved_call::artifact_cleanup_journal) fn rewrite(
        &self,
        token: &JournalToken,
        record: &ArtifactCleanupJournalRecord,
    ) -> anyhow::Result<()> {
        self.verify_path()?;
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
    pub(in crate::reserved_call::artifact_cleanup_journal) fn remove(
        &self,
        token: &JournalToken,
    ) -> anyhow::Result<()> {
        self.verify_path()?;
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
    pub(in crate::reserved_call::artifact_cleanup_journal) fn acquire_lease(
        &self,
        pending: &Path,
        claimed: &Path,
        needs_rename: bool,
    ) -> anyhow::Result<Option<std::fs::File>> {
        use fs2::FileExt as _;
        self.verify_path()?;
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
    pub(in crate::reserved_call::artifact_cleanup_journal) fn read(
        &self,
        path: &Path,
    ) -> anyhow::Result<Vec<u8>> {
        self.verify_path()?;
        Ok(std::fs::read(path)?)
    }
    pub(in crate::reserved_call::artifact_cleanup_journal) fn quarantine(
        &self,
        source: &Path,
        destination: &Path,
    ) -> anyhow::Result<()> {
        self.verify_path()?;
        std::fs::rename(source, destination)?;
        Ok(())
    }
    pub(in crate::reserved_call::artifact_cleanup_journal) fn write_owner(
        &self,
        claimed: &Path,
        bytes: &[u8],
    ) -> anyhow::Result<()> {
        self.verify_path()?;
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
