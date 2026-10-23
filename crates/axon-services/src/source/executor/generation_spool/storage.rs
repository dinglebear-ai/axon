//! Private, transactionally indexed generation records. Never a publication authority.
use super::MAX_WINDOW_BYTES;
use anyhow::{Context as _, ensure};
use fs2::FileExt as _;
use rusqlite::{Connection, OpenFlags, OptionalExtension as _, params};
use sha2::{Digest as _, Sha256};
use std::fs::{File, OpenOptions};
use std::path::Path;

const APPLICATION_ID: i32 = 0x41584e53;
const VERSION: i32 = 1;
const MAX_KEY_BYTES: usize = 4096;
const MAX_DATABASE_BYTES: u64 = 256 * 1024 * 1024;

pub(super) struct RecordStore {
    pub(super) db: Connection,
    // Field order closes SQLite before releasing the ownership fence.
    _owner: OwnerGuard,
}

pub(super) struct OwnerGuard(File);

impl Drop for OwnerGuard {
    fn drop(&mut self) {
        // A forked child may briefly inherit the open description before exec
        // applies CLOEXEC. Unlock explicitly after SQLite closes, rather than
        // waiting for the final inherited descriptor to close.
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Seal {
    pub(super) records: u64,
    pub(super) payload_bytes: u64,
    pub(super) digest: String,
}

impl RecordStore {
    pub(super) fn create(path: &Path, owner_key: &str) -> anyhow::Result<Self> {
        ensure!(
            !owner_key.is_empty() && owner_key.len() <= MAX_KEY_BYTES,
            "invalid generation spool owner identifier"
        );
        let owner = owner(path, true)?;
        drop(private_file(path, true)?);
        let db = connect(path)?;
        db.execute_batch(&format!(
            "PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version={VERSION};
             CREATE TABLE spool_meta(singleton INTEGER PRIMARY KEY CHECK(singleton=1), owner_key TEXT NOT NULL, seal_json TEXT);
             CREATE TABLE spool_records(sequence INTEGER PRIMARY KEY AUTOINCREMENT, record_key TEXT UNIQUE NOT NULL CHECK(length(CAST(record_key AS BLOB))<={MAX_KEY_BYTES}), payload BLOB NOT NULL CHECK(typeof(payload)='blob' AND length(payload)<={MAX_WINDOW_BYTES}), digest TEXT NOT NULL CHECK(length(CAST(digest AS BLOB))=64));"
        ))?;
        db.execute("INSERT INTO spool_meta VALUES(1,?,NULL)", [owner_key])?;
        // FULL-synchronous transactions and a directory sync establish a
        // recoverable receipt before the caller can acknowledge an append.
        sync_parent(path)?;
        let store = Self {
            db,
            _owner: OwnerGuard(owner),
        };
        store.validate(owner_key)?;
        Ok(store)
    }

    #[cfg(test)]
    pub(super) fn open(path: &Path, owner_key: &str) -> anyhow::Result<Self> {
        let owner = owner(path, false)?;
        let data = private_file(path, false)?;
        ensure!(
            data.metadata()?.len() <= MAX_DATABASE_BYTES,
            "generation spool database exceeds its 256 MiB disk limit"
        );
        // Closing another descriptor after SQLite starts can release POSIX
        // locks on that inode. Validate and close it before opening SQLite.
        drop(data);
        let store = Self {
            db: connect(path)?,
            _owner: OwnerGuard(owner),
        };
        store.validate(owner_key)?;
        Ok(store)
    }

    fn validate(&self, owner_key: &str) -> anyhow::Result<()> {
        let app: i32 = self
            .db
            .pragma_query_value(None, "application_id", |row| row.get(0))?;
        let version: i32 = self
            .db
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        ensure!(
            app == APPLICATION_ID && version == VERSION,
            "unsupported generation spool format; abandon uncommitted attempt"
        );
        let owner_bytes: u64 = self.db.query_row(
            "SELECT length(CAST(owner_key AS BLOB)) FROM spool_meta WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        ensure!(
            owner_bytes <= MAX_KEY_BYTES as u64,
            "generation spool owner identifier exceeds byte limit"
        );
        let actual: String = self.db.query_row(
            "SELECT owner_key FROM spool_meta WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        ensure!(
            actual == owner_key,
            "generation spool owner differs from requested source/job/generation/attempt"
        );
        let check: String = self
            .db
            .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
        ensure!(
            check == "ok",
            "generation spool SQLite integrity check failed; abandon uncommitted attempt"
        );
        if let Some(expected) = self.sealed()? {
            ensure!(
                self.visit_raw(|_, _, _| Ok(()))? == expected,
                "generation spool seal differs from stored records"
            );
        }
        Ok(())
    }

    pub(super) fn contains(&self, key: &str) -> anyhow::Result<bool> {
        ensure!(
            key.len() <= MAX_KEY_BYTES,
            "generation spool key exceeds 4096 bytes"
        );
        Ok(self
            .db
            .query_row(
                "SELECT 1 FROM spool_records WHERE record_key=?",
                [key],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub(super) fn append(&mut self, key: &str, payload: &[u8]) -> anyhow::Result<bool> {
        ensure!(
            key.len() <= MAX_KEY_BYTES && payload.len() <= MAX_WINDOW_BYTES,
            "generation spool record exceeds admitted key or payload byte window"
        );
        let digest = hex::encode(Sha256::digest(payload));
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let sealed: bool = tx.query_row(
            "SELECT seal_json IS NOT NULL FROM spool_meta WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        ensure!(!sealed, "generation spool is sealed; create a new attempt");
        let inserted = tx.execute("INSERT INTO spool_records(record_key,payload,digest) VALUES(?,?,?) ON CONFLICT(record_key) DO NOTHING", params![key,payload,digest])?;
        tx.commit()
            .context("generation spool commit failed; inspect persisted record before retrying")?;
        Ok(inserted != 0)
    }

    pub(super) fn sealed(&self) -> anyhow::Result<Option<Seal>> {
        let length: u64 = self.db.query_row(
            "SELECT COALESCE(length(seal_json),0) FROM spool_meta WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        ensure!(length <= 4096, "generation spool seal exceeds byte limit");
        let encoded: Option<String> = self.db.query_row(
            "SELECT seal_json FROM spool_meta WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        encoded
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }

    pub(super) fn seal(&mut self) -> anyhow::Result<Seal> {
        let observed = self.visit_raw(|_, _, _| Ok(()))?;
        if let Some(expected) = self.sealed()? {
            ensure!(
                expected == observed,
                "generation spool seal differs from stored records"
            );
            return Ok(expected);
        }
        self.db.execute(
            "UPDATE spool_meta SET seal_json=? WHERE singleton=1 AND seal_json IS NULL",
            [serde_json::to_string(&observed)?],
        )?;
        Ok(observed)
    }

    pub(super) fn visit_raw(
        &self,
        mut visit: impl FnMut(&str, &[u8], u64) -> anyhow::Result<()>,
    ) -> anyhow::Result<Seal> {
        let tx = self.db.unchecked_transaction()?;
        // Read lengths without materializing potentially corrupt large values.
        // The read transaction pins the same snapshot for the payload query.
        let mut statement = tx.prepare("SELECT sequence,length(CAST(record_key AS BLOB)),length(CAST(payload AS BLOB)),length(CAST(digest AS BLOB)) FROM spool_records ORDER BY sequence")?;
        let mut rows = statement.query([])?;
        let mut digest = Sha256::new();
        let mut records = 0_u64;
        let mut payload_bytes = 0_u64;
        while let Some(row) = rows.next()? {
            let sequence: i64 = row.get(0)?;
            let key_length: u64 = row.get(1)?;
            let payload_length: u64 = row.get(2)?;
            let digest_length: u64 = row.get(3)?;
            ensure!(
                sequence > 0
                    && key_length <= MAX_KEY_BYTES as u64
                    && payload_length <= MAX_WINDOW_BYTES as u64
                    && digest_length == 64,
                "generation spool record exceeds admitted key or payload byte window"
            );
            tx.query_row(
                "SELECT record_key,payload,digest FROM spool_records WHERE sequence=?",
                [sequence],
                |record| {
                    let key: String = record.get(0)?;
                    let payload = record.get_ref(1)?.as_blob()?;
                    let expected: String = record.get(2)?;
                    Ok((key, payload.to_vec(), expected))
                },
            )
            .map_err(anyhow::Error::from)
            .and_then(|(key, payload, expected)| {
                ensure!(
                    hex::encode(Sha256::digest(&payload)) == expected,
                    "generation spool record digest mismatch; abandon uncommitted attempt"
                );
                digest.update(sequence.to_le_bytes());
                digest.update(key_length.to_le_bytes());
                digest.update(key.as_bytes());
                digest.update(payload_length.to_le_bytes());
                digest.update(expected.as_bytes());
                visit(&key, &payload, sequence as u64)
            })?;
            records += 1;
            payload_bytes = payload_bytes
                .checked_add(payload_length)
                .context("generation spool byte accounting overflow")?;
        }
        Ok(Seal {
            records,
            payload_bytes,
            digest: hex::encode(digest.finalize()),
        })
    }
}

fn connect(path: &Path) -> anyhow::Result<Connection> {
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(std::time::Duration::from_secs(1))?;
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA cache_size=-4096; PRAGMA cache_spill=ON; PRAGMA mmap_size=0; PRAGMA temp_store=FILE; PRAGMA trusted_schema=OFF;")?;
    db.pragma_update(None, "max_page_count", MAX_DATABASE_BYTES / 4096)?;
    let page_size: u64 = db.pragma_query_value(None, "page_size", |row| row.get(0))?;
    ensure!(
        page_size == 4096,
        "generation spool page size differs from bounded disk accounting"
    );
    Ok(db)
}

// Caller holds the cross-process registry lock, which fences initialization.
// Existing and recovered work share exactly the same ownership inode.
pub(super) fn recovery_owner(path: &Path) -> anyhow::Result<OwnerGuard> {
    let owner_path = path.with_file_name(format!(
        "{}.owner",
        path.file_name()
            .context("missing spool filename")?
            .to_string_lossy()
    ));
    let create = match std::fs::symlink_metadata(&owner_path) {
        Ok(_) => false,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => return Err(error.into()),
    };
    Ok(OwnerGuard(owner(path, create)?))
}

fn owner(path: &Path, create: bool) -> anyhow::Result<File> {
    let name = path
        .file_name()
        .context("generation spool path has no filename")?;
    let mut owner_name = name.to_os_string();
    owner_name.push(".owner");
    let file = private_file(&path.with_file_name(owner_name), create)?;
    // A separate inode avoids interfering with SQLite's own locking on macOS.
    file.try_lock_exclusive()
        .context("generation spool already has an owner; do not recover an active attempt")?;
    Ok(file)
}

fn private_file(path: &Path, create: bool) -> anyhow::Result<File> {
    let parent = path
        .parent()
        .context("generation spool path has no parent")?;
    let metadata = std::fs::symlink_metadata(parent)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "generation spool parent must be a private directory"
    );
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
        ensure!(
            metadata.permissions().mode() & 0o077 == 0,
            "generation spool directory must be private"
        );
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "generation spool must be a regular file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        ensure!(
            file.metadata()?.permissions().mode() & 0o077 == 0,
            "generation spool file must be private"
        );
    }
    Ok(file)
}

fn sync_parent(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    File::open(
        path.parent()
            .context("generation spool path has no parent")?,
    )?
    .sync_all()?;
    Ok(())
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
