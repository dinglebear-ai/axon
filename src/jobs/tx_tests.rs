use super::*;
use crate::jobs::store::rollback_on_release;
use sqlx::sqlite::SqlitePoolOptions;

/// Single-slot in-memory pool wired with the same `after_release` ROLLBACK hook
/// the production pool uses. `max_connections(1)` keeps one connection (and thus
/// one in-memory database) alive across acquires, so the `t` table persists and
/// the slot-poisoning behaviour is observable.
async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .after_release(|conn, _meta| Box::pin(rollback_on_release(conn)))
        .connect(":memory:")
        .await
        .expect("pool");
    sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY)")
        .execute(&pool)
        .await
        .expect("create table");
    pool
}

async fn row_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM t")
        .fetch_one(pool)
        .await
        .expect("count")
}

#[tokio::test]
async fn commit_persists_writes() {
    let pool = test_pool().await;
    let mut tx = ImmediateTx::begin(&pool).await.expect("begin");
    sqlx::query("INSERT INTO t (id) VALUES (1)")
        .execute(tx.conn())
        .await
        .expect("insert");
    tx.commit().await.expect("commit");
    assert_eq!(row_count(&pool).await, 1, "committed row must persist");
}

#[tokio::test]
async fn rollback_discards_writes() {
    let pool = test_pool().await;
    let mut tx = ImmediateTx::begin(&pool).await.expect("begin");
    sqlx::query("INSERT INTO t (id) VALUES (1)")
        .execute(tx.conn())
        .await
        .expect("insert");
    tx.rollback().await;
    assert_eq!(row_count(&pool).await, 0, "rolled-back row must vanish");
}

/// The core safety property: a guard dropped WITHOUT commit/rollback must not
/// poison the single slot, and its writes must roll back. Without the
/// `after_release` net the next `BEGIN IMMEDIATE` would fail with
/// "within a transaction".
#[tokio::test]
async fn drop_unsettled_rolls_back_and_keeps_slot_usable() {
    let pool = test_pool().await;
    {
        let mut tx = ImmediateTx::begin(&pool).await.expect("begin");
        sqlx::query("INSERT INTO t (id) VALUES (1)")
            .execute(tx.conn())
            .await
            .expect("insert");
        // drop `tx` here WITHOUT commit/rollback — returns to the pool still
        // inside the transaction; the after_release hook must scrub it.
    }

    // Slot must not be poisoned: a fresh transaction begins cleanly.
    let tx2 = ImmediateTx::begin(&pool)
        .await
        .expect("slot must not be poisoned by an unsettled drop");
    tx2.rollback().await;

    // The unsettled write must have been rolled back, not silently committed.
    assert_eq!(
        row_count(&pool).await,
        0,
        "writes from an unsettled (dropped) transaction must not persist"
    );
}

#[tokio::test]
async fn finish_commits_on_ok() {
    let pool = test_pool().await;
    let mut tx = ImmediateTx::begin(&pool).await.expect("begin");
    let work: Result<i64, sqlx::Error> = async {
        sqlx::query("INSERT INTO t (id) VALUES (7)")
            .execute(tx.conn())
            .await?;
        Ok(7)
    }
    .await;
    let value = tx.finish(work).await.expect("finish ok");
    assert_eq!(value, 7);
    assert_eq!(row_count(&pool).await, 1, "finish(Ok) must commit");
}

#[tokio::test]
async fn finish_rolls_back_on_err() {
    let pool = test_pool().await;
    let mut tx = ImmediateTx::begin(&pool).await.expect("begin");
    let work: Result<i64, sqlx::Error> = async {
        sqlx::query("INSERT INTO t (id) VALUES (9)")
            .execute(tx.conn())
            .await?;
        // Force the work to fail after a write so finish() must roll back.
        Err(sqlx::Error::RowNotFound)
    }
    .await;
    let outcome = tx.finish(work).await;
    assert!(outcome.is_err(), "finish(Err) must propagate the error");
    assert_eq!(
        row_count(&pool).await,
        0,
        "finish(Err) must roll back the partial write"
    );
}
