use super::*;

#[tokio::test]
async fn default_pool_matches_unified_worker_fanout() {
    let pool = open_pool_unlocked(":memory:")
        .await
        .expect("open in-memory pool");
    assert_eq!(
        pool.options().get_max_connections(),
        DEFAULT_SQLITE_POOL_CONNECTIONS
    );
    assert_eq!(DEFAULT_SQLITE_POOL_CONNECTIONS, 8);
}

#[tokio::test]
async fn wal_sidecars_survive_the_last_pool_close() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("jobs.db");
    let pool = open_pool_unlocked(db.to_str().expect("utf8 path"))
        .await
        .expect("open pool");

    sqlx::query("CREATE TABLE proof (value INTEGER NOT NULL)")
        .execute(&pool)
        .await
        .expect("create table");
    sqlx::query("INSERT INTO proof (value) VALUES (1)")
        .execute(&pool)
        .await
        .expect("write WAL");
    pool.close().await;

    assert!(
        PathBuf::from(format!("{}-wal", db.display())).exists(),
        "PERSIST_WAL must keep the WAL pathname linked after pool close"
    );
    assert!(
        PathBuf::from(format!("{}-shm", db.display())).exists(),
        "PERSIST_WAL must keep the shared-memory pathname linked after pool close"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn opening_existing_database_repairs_database_and_sidecar_modes() {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("jobs.db");
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .mode(0o666)
        .open(&db)
        .unwrap();
    std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o666)).unwrap();
    let pool = open_pool_unlocked(db.to_str().unwrap()).await.unwrap();
    sqlx::query("CREATE TABLE permission_proof (value INTEGER)")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        std::fs::metadata(&db).unwrap().permissions().mode() & 0o777,
        0o600
    );
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", db.display()));
        assert_eq!(
            std::fs::metadata(sidecar).unwrap().permissions().mode() & 0o077,
            0,
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn sqlite_open_rejects_symlink_target_before_connecting() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.db");
    let link = dir.path().join("jobs.db");
    std::fs::write(&target, b"").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let error = open_pool_unlocked(link.to_str().unwrap())
        .await
        .expect_err("O_NOFOLLOW must reject a database symlink");
    assert!(error.to_string().contains("refusing to open database"));
}

#[test]
fn corruption_recovery_preserves_originals_when_sidecar_backup_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("jobs.db");
    std::fs::write(&db, b"database-bytes").unwrap();
    std::fs::create_dir(format!("{}-wal", db.display())).unwrap();

    let error = recover_corrupted_database(&db, "test corruption")
        .expect_err("unpreservable WAL must abort recovery");
    assert!(error.to_string().contains("preserve"));
    assert_eq!(std::fs::read(&db).unwrap(), b"database-bytes");
    assert!(PathBuf::from(format!("{}-wal", db.display())).is_dir());
}

#[cfg(unix)]
#[tokio::test]
async fn external_reader_churn_keeps_wal_generation_linked_and_visible() {
    use std::os::unix::fs::MetadataExt;
    use std::process::Command;

    let sqlite3_available = Command::new("sqlite3")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    assert!(
        sqlite3_available,
        "sqlite3 CLI is required for the cross-process WAL regression"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("jobs.db");
    let pool = open_pool_unlocked(db.to_str().expect("utf8 path"))
        .await
        .expect("open long-lived pool");
    sqlx::query("CREATE TABLE proof (value INTEGER NOT NULL)")
        .execute(&pool)
        .await
        .expect("create table");
    sqlx::query("INSERT INTO proof (value) VALUES (0)")
        .execute(&pool)
        .await
        .expect("seed");

    let wal = PathBuf::from(format!("{}-wal", db.display()));
    let shm = PathBuf::from(format!("{}-shm", db.display()));
    let wal_inode = std::fs::metadata(&wal).expect("WAL metadata").ino();
    let shm_inode = std::fs::metadata(&shm).expect("SHM metadata").ino();

    for expected in 1..=20i64 {
        let output = Command::new("sqlite3")
            .arg(&db)
            .arg("SELECT value FROM proof")
            .output()
            .expect("spawn external sqlite reader");
        assert!(
            output.status.success(),
            "external read failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        sqlx::query("UPDATE proof SET value = ?")
            .bind(expected)
            .execute(&pool)
            .await
            .expect("long-lived writer remains usable");

        let wal_meta = std::fs::metadata(&wal).expect("WAL must remain linked");
        let shm_meta = std::fs::metadata(&shm).expect("SHM must remain linked");
        assert_eq!(wal_meta.ino(), wal_inode, "WAL generation changed");
        assert_eq!(shm_meta.ino(), shm_inode, "SHM generation changed");
        assert!(wal_meta.nlink() > 0, "WAL was unlinked");
        assert!(shm_meta.nlink() > 0, "SHM was unlinked");

        let observed: i64 = String::from_utf8(
            Command::new("sqlite3")
                .arg(&db)
                .arg("SELECT value FROM proof")
                .output()
                .expect("spawn visibility reader")
                .stdout,
        )
        .expect("utf8 sqlite output")
        .trim()
        .parse()
        .expect("integer sqlite output");
        assert_eq!(observed, expected, "external reader saw stale WAL state");
    }
}
