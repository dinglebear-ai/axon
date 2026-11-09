//! Durable conservative quota reservations. Caller holds the cross-process disk fence.
//! Reconcile once per process/open; per-page admission is indexed O(log pages).
use anyhow::{Context as _, ensure};
use rusqlite::{Connection, params};
use std::path::Path;
const NAME: &str = ".quota.sqlite";
const MAX_DATABASE_BYTES: u64 = 16 * 1024 * 1024;
// Canonical and initializing databases/journals plus one staged payload.
// Abandoned initializer files are retained and charged by reconciliation.
pub(super) const DISK_OVERHEAD: u64 = 4 * MAX_DATABASE_BYTES + super::MAX_OUTBOX_FILE_BYTES;
const APPLICATION_ID: i32 = 0x41584f51;
#[cfg(test)]
thread_local! { static RECONCILIATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

pub(super) fn open(root: &Path, reconcile: bool) -> anyhow::Result<Connection> {
    let path = root.join(NAME);
    let exists = path.try_exists()?;
    if !exists {
        initialize(root, &path)?;
    }
    let metadata = std::fs::symlink_metadata(&path)?;
    ensure!(
        metadata.is_file()
            && metadata.len() > 0
            && !metadata.file_type().is_symlink()
            && metadata.len() <= MAX_DATABASE_BYTES,
        "invalid outbox quota database; preserve and reconcile before retry"
    );
    let mut db = connect(&path)?;
    let app: i32 = db.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let version: i32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        app == APPLICATION_ID && version == 1,
        "unsupported outbox quota format; preserve for reconciliation"
    );
    // Validate required tables before adding the compatible paging index.
    // Unknown/partial canonical files remain untouched rather than repaired.
    let _: u64 = db.query_row("SELECT bytes FROM usage WHERE singleton=1", [], |row| {
        row.get(0)
    })?;
    db.prepare("SELECT name,bytes FROM reservations LIMIT 0")?;
    db.execute_batch("CREATE INDEX IF NOT EXISTS reservations_pending_names ON reservations(name) WHERE substr(name,-5)='.json';")?;
    if reconcile || !exists {
        #[cfg(test)]
        RECONCILIATIONS.with(|count| count.set(count.get() + 1));
        let check: String = db.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
        ensure!(
            check == "ok",
            "corrupt outbox quota database; preserve for reconciliation"
        );
        let tx = db.transaction()?;
        tx.execute("DELETE FROM reservations", [])?;
        let mut used = 0_u64;
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .context("non-UTF8 outbox entry; preserve for inspection")?;
            if name == NAME || name == ".quota.sqlite-journal" {
                continue;
            }
            let meta = std::fs::symlink_metadata(entry.path())?;
            ensure!(
                meta.is_file() || (meta.file_type().is_symlink() && name.ends_with(".json")),
                "outbox quota reconciliation found unsupported nonregular entry"
            );
            used = used
                .checked_add(meta.len())
                .context("outbox quota overflow")?;
            tx.execute(
                "INSERT INTO reservations VALUES(?,?)",
                params![name, meta.len()],
            )?;
        }
        tx.execute("UPDATE usage SET bytes=? WHERE singleton=1", [used])?;
        tx.commit()?;
        std::fs::File::open(root)?.sync_all()?;
    }
    Ok(db)
}
// Publish only a complete, closed, synced SQLite database. The caller holds
// the disk fence. A crash before publish leaves ordinary private files, which
// reconciliation preserves and charges rather than mistaking them for NAME.
fn initialize(root: &Path, path: &Path) -> anyhow::Result<()> {
    let temporary = tempfile::Builder::new()
        .prefix(".quota-init-")
        .suffix(".sqlite")
        .tempfile_in(root)?;
    let mut db = connect(temporary.path())?;
    let tx = db.transaction()?;
    tx.execute_batch(&format!(
        "PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version=1;
         CREATE TABLE reservations(name TEXT PRIMARY KEY, bytes INTEGER NOT NULL CHECK(bytes>=0));
         CREATE TABLE usage(singleton INTEGER PRIMARY KEY CHECK(singleton=1), bytes INTEGER NOT NULL CHECK(bytes>=0));
         INSERT INTO usage VALUES(1,0);"
    ))?;
    tx.commit()?;
    db.close().map_err(|(_, error)| error)?;
    temporary.as_file().sync_all()?;
    // No-clobber publication preserves an unexpectedly existing/unknown target.
    temporary
        .persist_noclobber(path)
        .map_err(|error| error.error)?;
    std::fs::File::open(root)?.sync_all()?;
    Ok(())
}

fn connect(path: &Path) -> anyhow::Result<Connection> {
    let db = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA mmap_size=0; PRAGMA cache_size=-1024; PRAGMA trusted_schema=OFF;")?;
    let page_size: u64 = db.pragma_query_value(None, "page_size", |row| row.get(0))?;
    db.pragma_update(None, "max_page_count", MAX_DATABASE_BYTES / page_size)?;
    Ok(db)
}

fn name(path: &Path) -> anyhow::Result<&str> {
    path.file_name()
        .and_then(|name| name.to_str())
        .context("invalid outbox quota key")
}
pub(super) fn reserve(
    db: &mut Connection,
    path: &Path,
    incoming: u64,
    limit: u64,
) -> anyhow::Result<()> {
    let tx = db.transaction()?;
    let used: u64 = tx.query_row("SELECT bytes FROM usage WHERE singleton=1", [], |row| {
        row.get(0)
    })?;
    let previous: u64 = tx.query_row(
        "SELECT COALESCE((SELECT bytes FROM reservations WHERE name=?),0)",
        [name(path)?],
        |row| row.get(0),
    )?;
    let reserved = previous.max(incoming);
    let next = used
        .checked_add(reserved - previous)
        .context("outbox quota overflow")?;
    ensure!(
        next <= limit,
        "artifact candidate outbox disk budget exceeded: reserved={next}, limit={limit}; drain deliveries or reconcile retained private files before retry"
    );
    tx.execute(
        "INSERT INTO reservations VALUES(?,?) ON CONFLICT(name) DO UPDATE SET bytes=excluded.bytes",
        params![name(path)?, reserved],
    )?;
    tx.execute("UPDATE usage SET bytes=? WHERE singleton=1", [next])?;
    tx.commit()?;
    Ok(())
}
pub(super) fn release(db: &mut Connection, path: &Path) -> anyhow::Result<()> {
    let tx = db.transaction()?;
    let previous: u64 = tx.query_row(
        "SELECT COALESCE((SELECT bytes FROM reservations WHERE name=?),0)",
        [name(path)?],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE usage SET bytes=bytes-? WHERE singleton=1",
        [previous],
    )?;
    tx.execute("DELETE FROM reservations WHERE name=?", [name(path)?])?;
    tx.commit()?;
    Ok(())
}

// Reserve both identities before renaming: interruption may overcharge, never
// leave quarantined bytes available for a same-key retry to reuse uncharged.
pub(super) fn quarantine_file(path: &Path, target: &Path, limit: u64) -> anyhow::Result<()> {
    let root = path.parent().context("outbox quarantine has no parent")?;
    let _owner = lock_disk_admission(root)?;
    let mut db = open(root, false)?;
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "outbox quarantine requires a regular file"
    );
    reserve(&mut db, target, metadata.len(), limit)?;
    std::fs::rename(path, target)?;
    std::fs::File::open(root)?.sync_all()?;
    release(&mut db, path)?;
    Ok(())
}

pub(super) struct OutboxDiskOwner(std::fs::File);
impl Drop for OutboxDiskOwner {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}
pub(super) fn lock_disk_admission(root: &std::path::Path) -> anyhow::Result<OutboxDiskOwner> {
    use fs2::FileExt as _;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(root.join(".admission.lock"))?;
    anyhow::ensure!(
        file.metadata()?.is_file(),
        "outbox admission owner must be a regular file"
    );
    file.try_lock_exclusive()?;
    Ok(OutboxDiskOwner(file))
}

pub(super) fn bounded_encode(value: &impl serde::Serialize, limit: u64) -> anyhow::Result<Vec<u8>> {
    struct Counter {
        bytes: u64,
        limit: u64,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let next = self
                .bytes
                .checked_add(bytes.len() as u64)
                .filter(|next| *next <= self.limit)
                .ok_or_else(|| {
                    std::io::Error::other("artifact candidate outbox byte limit exceeded")
                })?;
            self.bytes = next;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { bytes: 0, limit };
    serde_json::to_writer(&mut counter, value)?;
    let mut bytes = Vec::with_capacity(usize::try_from(counter.bytes)?);
    serde_json::to_writer(&mut bytes, value)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_initializers_are_preserved_and_charged_without_poisoning_open() {
        let temp = tempfile::tempdir().unwrap();
        let empty = temp.path().join(".quota-init-empty.sqlite");
        std::fs::write(&empty, []).unwrap();
        let partial = temp.path().join(".quota-init-partial.sqlite");
        let interrupted = Connection::open(&partial).unwrap();
        interrupted
            .execute_batch(&format!(
                "PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version=1;
             CREATE TABLE reservations(name TEXT PRIMARY KEY, bytes INTEGER NOT NULL);"
            ))
            .unwrap();
        drop(interrupted);
        let expected = std::fs::metadata(&partial).unwrap().len();
        let payload = temp.path().join("pending.json");
        std::fs::write(&payload, b"pending").unwrap();
        let _owner = lock_disk_admission(temp.path()).unwrap();
        let mut db = open(temp.path(), true).unwrap();
        let used: u64 = db
            .query_row("SELECT bytes FROM usage WHERE singleton=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        // The admission lock is also a regular zero-byte reconciled file.
        assert_eq!(used, expected + 7);
        assert!(reserve(&mut db, &temp.path().join("next.json"), 1, used).is_err());
        assert!(empty.exists() && partial.exists());
        assert_eq!(std::fs::read(payload).unwrap(), b"pending");
        drop(db);
        drop(open(temp.path(), true).unwrap());
    }

    #[test]
    fn unknown_empty_and_partial_canonical_databases_remain_fail_closed() {
        for partial in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join(NAME);
            if partial {
                let db = Connection::open(&path).unwrap();
                db.execute_batch(&format!(
                    "PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version=1;
                     CREATE TABLE reservations(name TEXT PRIMARY KEY, bytes INTEGER NOT NULL);"
                ))
                .unwrap();
            } else {
                std::fs::write(&path, []).unwrap();
            }
            let before = std::fs::read(&path).unwrap();
            let payload = temp.path().join("pending.json");
            std::fs::write(&payload, b"pending").unwrap();
            assert!(open(temp.path(), true).is_err());
            assert_eq!(std::fs::read(path).unwrap(), before);
            assert_eq!(std::fs::read(payload).unwrap(), b"pending");
        }
    }

    #[test]
    fn legacy_quarantine_and_cancelled_reservations_reconcile_without_undercharging() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("legacy.invalid"), b"123456").unwrap();
        let path = temp.path().join("delivery.json");
        let mut db = open(temp.path(), true).unwrap();
        assert!(reserve(&mut db, &path, 5, 10).is_err());
        reserve(&mut db, &path, 4, 10).unwrap();
        reserve(&mut db, &path, 4, 10).unwrap();
        assert!(reserve(&mut db, &temp.path().join("other.json"), 1, 10).is_err());
        drop(db); // interrupted writer never wrote delivery: reopen reclaims reservation
        let mut db = open(temp.path(), true).unwrap();
        reserve(&mut db, &path, 4, 10).unwrap();
        release(&mut db, &path).unwrap();
        assert!(reserve(&mut db, &path, 5, 10).is_err());
    }
    #[test]
    fn quarantine_keeps_bytes_charged_for_same_key_retry_and_crash_reservations() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("delivery.json");
        let target = temp.path().join("delivery.invalid");
        std::fs::write(&path, b"123456").unwrap();
        let mut db = open(temp.path(), true).unwrap();
        drop(db);
        quarantine_file(&path, &target, 12).unwrap();
        db = open(temp.path(), false).unwrap();
        reserve(&mut db, &path, 6, 12).unwrap();
        assert!(reserve(&mut db, &temp.path().join("overflow.json"), 1, 12).is_err());
        // A cancelled retry has no file but retains its charge until owned reconciliation.
        drop(db);
        db = open(temp.path(), true).unwrap();
        assert!(reserve(&mut db, &path, 7, 12).is_err());
        reserve(&mut db, &path, 6, 12).unwrap();
    }
    #[test]
    fn quarantine_at_capacity_preserves_original_and_corrupt_quota_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("delivery.json");
        let target = temp.path().join("delivery.invalid");
        std::fs::write(&path, b"123456").unwrap();
        drop(open(temp.path(), true).unwrap());
        assert!(quarantine_file(&path, &target, 6).is_err());
        assert!(path.exists());
        assert!(!target.exists());
        std::fs::write(temp.path().join(NAME), b"corrupt").unwrap();
        assert!(quarantine_file(&path, &target, 12).is_err());
        assert!(path.exists());
        assert!(!target.exists());
    }
    #[test]
    fn many_page_admissions_use_the_persisted_counter_and_reject_corruption() {
        let temp = tempfile::tempdir().unwrap();
        RECONCILIATIONS.with(|count| count.set(0));
        let mut db = open(temp.path(), true).unwrap();
        for index in 0..2000 {
            drop(db);
            db = open(temp.path(), false).unwrap();
            reserve(&mut db, &temp.path().join(format!("{index}.json")), 1, 2000).unwrap();
        }
        assert_eq!(
            RECONCILIATIONS.with(|count| count.get()),
            1,
            "page admissions must not rescan the directory"
        );
        assert!(reserve(&mut db, &temp.path().join("overflow.json"), 1, 2000).is_err());
        drop(db);
        std::fs::write(temp.path().join(NAME), b"corrupt").unwrap();
        assert!(open(temp.path(), true).is_err());
    }
}

/// Bounded lexical inventory. `reservations_pending_names` is a partial index
/// installed by open() for exactly this predicate, including upgraded stores.
/// The caller holds the cross-process disk fence through the whole read.
pub(super) fn inventory_page(
    db: &Connection,
    cursor: &str,
    limit: usize,
) -> anyhow::Result<(Vec<String>, bool)> {
    ensure!(
        limit > 0 && limit <= super::MAX_PENDING_DELIVERIES,
        "invalid outbox inventory page size"
    );
    let mut statement = db.prepare(
        "SELECT name FROM reservations WHERE substr(name,-5)='.json' AND name>?1 ORDER BY name LIMIT ?2",
    )?;
    let mut rows = statement.query(params![cursor, (limit + 1) as i64])?;
    let mut names = Vec::with_capacity(limit);
    let mut has_more = false;
    while let Some(row) = rows.next()? {
        let name = row.get_ref(0)?.as_str()?;
        let path = Path::new(name);
        ensure!(
            name.len() <= 4096
                && path.components().count() == 1
                && matches!(
                    path.components().next(),
                    Some(std::path::Component::Normal(_))
                ),
            "invalid outbox quota inventory name; preserve database for reconciliation"
        );
        if names.len() == limit {
            has_more = true;
            break;
        }
        names.push(name.to_owned());
    }
    Ok((names, has_more))
}

#[cfg(test)]
#[path = "quota_inventory_tests.rs"]
mod inventory_tests;
