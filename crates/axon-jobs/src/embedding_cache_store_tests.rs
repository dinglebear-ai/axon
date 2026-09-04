use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;

use axon_api::source::ProviderKind;
use axon_embedding::cache::EmbeddingVectorCacheStore;
use sqlx::sqlite::SqlitePoolOptions;

use super::*;
use crate::scheduler::{ProviderCapacityDomain, ProviderScheduler, SchedulerConfig};
use crate::store::open_sqlite_pool;

async fn assert_pending<F: Future>(mut future: Pin<&mut F>, message: &str) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending(), "{message}");
        Poll::Ready(())
    })
    .await;
}

async fn store() -> (SqliteEmbeddingVectorCacheStore, SqlitePool, SqliteWriteGate) {
    let pool = open_sqlite_pool(":memory:").await.expect("cache database");
    let gate = SqliteWriteGate::default();
    (
        SqliteEmbeddingVectorCacheStore::new_without_maintenance(
            pool.clone(),
            gate.clone(),
            100_000,
        ),
        pool,
        gate,
    )
}

fn entry(index: usize) -> CachedEmbedding {
    CachedEmbedding {
        cache_key: format!("sha256:{index:064x}"),
        provider_id: ProviderId::new("tei"),
        model: "test-model".into(),
        dimensions: 4,
        values: vec![index as f32; 4],
    }
}

#[tokio::test]
async fn max_configured_batch_is_chunked_below_sqlite_bind_budget() {
    let (store, pool, _) = store().await;
    let entries = (0..65_536).map(entry).collect::<Vec<_>>();

    store.put_many(&entries, 100_000).await.expect("bulk write");
    let keys = entries
        .iter()
        .map(|entry| entry.cache_key.clone())
        .collect::<Vec<_>>();
    let lookup = store.get_many(&keys, 4).await.expect("bulk read");

    assert_eq!(lookup.hits.len(), entries.len());
    assert!(lookup.corrupt_entries.is_empty());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embedding_vector_cache")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, entries.len() as i64);
}

#[tokio::test]
async fn corrupt_rows_are_reported_and_can_be_retired() {
    let (store, pool, _) = store().await;
    let key = entry(1).cache_key;
    sqlx::query(
        "INSERT INTO embedding_vector_cache \
         (cache_key, provider_id, model, dimensions, vector, created_at, last_used_at) \
         VALUES (?, 'tei', 'test-model', 4, X'00', 0, 0)",
    )
    .bind(&key)
    .execute(&pool)
    .await
    .unwrap();

    let lookup = store.get_many(std::slice::from_ref(&key), 4).await.unwrap();
    assert!(lookup.hits.is_empty());
    assert_eq!(lookup.corrupt_entries[0].cache_key, key);

    store.retire_many(&lookup.corrupt_entries).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embedding_vector_cache")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn non_finite_vectors_are_reported_as_corrupt() {
    let (store, pool, _) = store().await;
    let nan_key = entry(2).cache_key;
    let infinite_key = entry(3).cache_key;
    for (key, value) in [(&nan_key, f32::NAN), (&infinite_key, f32::INFINITY)] {
        let values = [0.0_f32, value, 1.0, 2.0];
        let bytes = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        sqlx::query(
            "INSERT INTO embedding_vector_cache \
             (cache_key, provider_id, model, dimensions, vector, created_at, last_used_at) \
             VALUES (?, 'tei', 'test-model', 4, ?, 0, 0)",
        )
        .bind(key)
        .bind(bytes)
        .execute(&pool)
        .await
        .unwrap();
    }

    let lookup = store
        .get_many(&[nan_key.clone(), infinite_key.clone()], 4)
        .await
        .unwrap();

    assert!(lookup.hits.is_empty());
    assert_eq!(
        lookup
            .corrupt_entries
            .iter()
            .map(|entry| entry.cache_key.clone())
            .collect::<Vec<_>>(),
        vec![nan_key, infinite_key]
    );
}

#[tokio::test]
async fn retention_prunes_deterministically_after_chunked_writes() {
    let (store, pool, _) = store().await;
    let entries = (0..1_000).map(entry).collect::<Vec<_>>();

    store.put_many(&entries, 250).await.unwrap();
    for _ in 0..2 {
        run_periodic_maintenance(&store.inner).await;
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embedding_vector_cache")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 250);
    let first: String = sqlx::query_scalar(
        "SELECT cache_key FROM embedding_vector_cache ORDER BY cache_key LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(first, entry(750).cache_key);
}

#[tokio::test]
async fn expired_entries_miss_and_are_reported_for_lazy_retirement() {
    let (store, pool, _) = store().await;
    let expired = entry(42);
    let created_at = chrono::Utc::now().timestamp_millis() - MAX_CACHE_AGE.as_millis() as i64 - 1;
    sqlx::query(
        "INSERT INTO embedding_vector_cache \
         (cache_key, provider_id, model, dimensions, vector, created_at, last_used_at) \
         VALUES (?, 'tei', 'test-model', 4, ?, ?, ?)",
    )
    .bind(&expired.cache_key)
    .bind(encode_vector(&expired.values))
    .bind(created_at)
    .bind(created_at)
    .execute(&pool)
    .await
    .unwrap();

    let lookup = store
        .get_many(std::slice::from_ref(&expired.cache_key), 4)
        .await
        .unwrap();

    assert!(lookup.hits.is_empty());
    assert_eq!(lookup.corrupt_entries[0].cache_key, expired.cache_key);
}

#[tokio::test]
async fn stale_retirement_cannot_delete_a_recomputed_entry() {
    let (store, pool, _) = store().await;
    let mut replacement = entry(43);
    let expired_at = chrono::Utc::now().timestamp_millis() - MAX_CACHE_AGE.as_millis() as i64 - 1;
    sqlx::query(
        "INSERT INTO embedding_vector_cache \
         (cache_key, provider_id, model, dimensions, vector, created_at, last_used_at) \
         VALUES (?, 'tei', 'test-model', 4, ?, ?, ?)",
    )
    .bind(&replacement.cache_key)
    .bind(encode_vector(&replacement.values))
    .bind(expired_at)
    .bind(expired_at)
    .execute(&pool)
    .await
    .unwrap();

    let stale = store
        .get_many(std::slice::from_ref(&replacement.cache_key), 4)
        .await
        .unwrap();
    replacement.values = vec![99.0; 4];
    store
        .put_many(std::slice::from_ref(&replacement), 100)
        .await
        .unwrap();
    let refreshed_at: i64 =
        sqlx::query_scalar("SELECT created_at FROM embedding_vector_cache WHERE cache_key = ?")
            .bind(&replacement.cache_key)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(refreshed_at > expired_at);
    store.retire_many(&stale.corrupt_entries).await.unwrap();

    let lookup = store
        .get_many(std::slice::from_ref(&replacement.cache_key), 4)
        .await
        .unwrap();
    assert_eq!(
        lookup.hits[&replacement.cache_key].values,
        replacement.values
    );
}

#[tokio::test]
async fn corrupt_row_retirement_cannot_delete_a_recomputed_entry() {
    let (store, pool, _) = store().await;
    let replacement = entry(44);
    let created_at = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO embedding_vector_cache \
         (cache_key, provider_id, model, dimensions, vector, created_at, last_used_at) \
         VALUES (?, 'tei', 'test-model', 4, X'00', ?, ?)",
    )
    .bind(&replacement.cache_key)
    .bind(created_at)
    .bind(created_at)
    .execute(&pool)
    .await
    .unwrap();

    let stale = store
        .get_many(std::slice::from_ref(&replacement.cache_key), 4)
        .await
        .unwrap();
    store
        .put_many(std::slice::from_ref(&replacement), 100)
        .await
        .unwrap();
    let refreshed_at: i64 =
        sqlx::query_scalar("SELECT created_at FROM embedding_vector_cache WHERE cache_key = ?")
            .bind(&replacement.cache_key)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(refreshed_at > created_at);
    store.retire_many(&stale.corrupt_entries).await.unwrap();

    let lookup = store
        .get_many(std::slice::from_ref(&replacement.cache_key), 4)
        .await
        .unwrap();
    assert_eq!(
        lookup.hits[&replacement.cache_key].values,
        replacement.values
    );
}

#[tokio::test]
async fn successful_touch_updates_recency_and_protects_a_hot_entry() {
    let (store, pool, _) = store().await;
    let initial = [entry(50), entry(51), entry(52)];
    store.put_many(&initial, 3).await.unwrap();
    let base = chrono::Utc::now().timestamp_millis() - 10_000;
    for (offset, item) in initial.iter().enumerate() {
        sqlx::query("UPDATE embedding_vector_cache SET last_used_at = ? WHERE cache_key = ?")
            .bind(base + offset as i64)
            .bind(&item.cache_key)
            .execute(&pool)
            .await
            .unwrap();
    }

    store
        .touch_many(std::slice::from_ref(&initial[0].cache_key))
        .await
        .unwrap();
    store.put_many(&[entry(53)], 3).await.unwrap();

    let survivors: Vec<String> =
        sqlx::query_scalar("SELECT cache_key FROM embedding_vector_cache ORDER BY cache_key")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(survivors.contains(&initial[0].cache_key));
    assert!(!survivors.contains(&initial[1].cache_key));
    let hit_count: i64 =
        sqlx::query_scalar("SELECT hit_count FROM embedding_vector_cache WHERE cache_key = ?")
            .bind(&initial[0].cache_key)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(hit_count, 1);
}

#[tokio::test]
async fn warm_hit_touch_skips_a_busy_writer_gate() {
    let (store, _pool, gate) = store().await;
    let held = gate.lock().await;

    tokio::time::timeout(
        Duration::from_millis(20),
        store.touch_many(&[entry(1).cache_key]),
    )
    .await
    .expect("touch must not wait behind the shared writer gate")
    .unwrap();
    drop(held);
}

#[tokio::test]
async fn mutation_deadline_applies_only_to_writer_admission() {
    let (store, pool, gate) = store().await;
    let held = gate.lock().await;

    let error = store
        .put_many(&[entry(7)], 100)
        .await
        .expect_err("busy admission must be bounded");
    assert!(error.to_string().contains("admission timed out"));
    drop(held);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embedding_vector_cache")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "timed-out admission must not begin a transaction");
    store.put_many(&[entry(7)], 100).await.unwrap();
}

#[tokio::test]
async fn timed_out_wait_keeps_writer_gate_until_real_mutation_finishes() {
    let directory = tempfile::tempdir().expect("cache database directory");
    let database = directory.path().join("cache.db");
    let pool = open_sqlite_pool(database.to_str().expect("UTF-8 database path"))
        .await
        .expect("cache database");
    let gate = SqliteWriteGate::default();
    let store =
        SqliteEmbeddingVectorCacheStore::new_without_maintenance(pool.clone(), gate.clone(), 100);

    let mut external_writer = pool.acquire().await.expect("external writer connection");
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *external_writer)
        .await
        .expect("hold SQLite writer lock");

    let mut mutation = tokio::spawn({
        let store = store.clone();
        async move { store.put_many(&[entry(17)], 100).await }
    });
    poll_fn(|cx| {
        if gate.try_lock().is_none() {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;

    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut mutation)
            .await
            .is_err(),
        "the caller deadline must expire while the real SQLite mutation remains owned"
    );
    let mut competing_writer = Box::pin(gate.lock());
    assert_pending(
        competing_writer.as_mut(),
        "the admitted mutation must retain writer admission after caller timeout",
    )
    .await;

    sqlx::query("COMMIT")
        .execute(&mut *external_writer)
        .await
        .expect("release SQLite writer lock");
    mutation
        .await
        .expect("mutation task")
        .expect("mutation completes after lock release");
    drop(competing_writer.await);
}

#[tokio::test]
async fn maintenance_uses_fixed_size_passes_and_exact_trigger_count() {
    let (store, pool, _) = store().await;
    let entries = (0..1_200).map(entry).collect::<Vec<_>>();
    store.put_many(&entries, 10).await.unwrap();
    let after_first_pass: i64 =
        sqlx::query_scalar("SELECT entry_count FROM embedding_vector_cache_state")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after_first_pass, 1_200 - MAINTENANCE_DELETE_BUDGET);

    run_periodic_maintenance(&store.inner).await;
    run_periodic_maintenance(&store.inner).await;
    let after: i64 = sqlx::query_scalar("SELECT entry_count FROM embedding_vector_cache_state")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, 10);
}

#[tokio::test]
async fn periodic_maintenance_reclaims_expired_rows_without_cache_traffic() {
    let pool = open_sqlite_pool(":memory:").await.expect("cache database");
    let store =
        SqliteEmbeddingVectorCacheStore::new(pool.clone(), SqliteWriteGate::default(), 100_000);
    let expired = entry(91);
    sqlx::query(
        "INSERT INTO embedding_vector_cache \
         (cache_key, provider_id, model, dimensions, vector, created_at, last_used_at) \
         VALUES (?, 'tei', 'test-model', 4, ?, 0, 0)",
    )
    .bind(&expired.cache_key)
    .bind(encode_vector(&expired.values))
    .execute(&pool)
    .await
    .unwrap();

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let count: i64 =
                sqlx::query_scalar("SELECT entry_count FROM embedding_vector_cache_state")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            if count == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("periodic cache maintenance must make progress");
    drop(store);
}

#[tokio::test]
async fn periodic_maintenance_uses_configured_cap_before_any_put() {
    let pool = open_sqlite_pool(":memory:").await.expect("cache database");
    let store = SqliteEmbeddingVectorCacheStore::new(pool.clone(), SqliteWriteGate::default(), 5);
    let now = chrono::Utc::now().timestamp_millis();
    for index in 0..20 {
        let value = entry(index);
        sqlx::query(
            "INSERT INTO embedding_vector_cache \
             (cache_key, provider_id, model, dimensions, vector, created_at, last_used_at) \
             VALUES (?, 'tei', 'test-model', 4, ?, ?, ?)",
        )
        .bind(value.cache_key)
        .bind(encode_vector(&value.values))
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
    }

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let count: i64 =
                sqlx::query_scalar("SELECT entry_count FROM embedding_vector_cache_state")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            if count == 5 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("configured cap must apply before cache writes");
    drop(store);
}

#[tokio::test]
async fn cache_and_scheduler_share_writer_admission_before_pool_acquisition() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("single-connection cache database");
    crate::migrations::apply_all_migrations(&pool)
        .await
        .expect("cache schema");
    let gate = SqliteWriteGate::default();
    let store = SqliteEmbeddingVectorCacheStore::new(pool.clone(), gate.clone(), 100_000);
    let mut only_connection = pool.acquire().await.expect("only pool connection");
    let held_gate = gate.lock().await;

    let scheduler = ProviderScheduler::new_with_write_gate(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "tei".into(),
            authority_id: "test".into(),
        },
        SchedulerConfig::new(1, 0, 16, 16).expect("valid scheduler configuration"),
        gate.clone(),
    )
    .unwrap();
    let entries = [entry(1)];
    let mut cache_write = Box::pin(store.put_many(&entries, 100));
    let mut scheduler_write = Box::pin(scheduler.reconcile());
    assert_pending(
        cache_write.as_mut(),
        "cache writer must wait while shared admission is held",
    )
    .await;
    assert_pending(
        scheduler_write.as_mut(),
        "scheduler writer must wait while shared admission is held",
    )
    .await;
    // SQLx normally returns dropped connections from a spawned task. Drive
    // that handoff explicitly so this assertion has no scheduler/yield race.
    only_connection.return_to_pool().await;
    drop(
        pool.try_acquire()
            .expect("both gate waiters must leave the only pool connection available"),
    );
    drop(held_gate);
    cache_write.await.unwrap();
    scheduler_write.await.unwrap();
}

#[tokio::test]
async fn missing_cache_schema_surfaces_a_store_error() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let store = SqliteEmbeddingVectorCacheStore::new(pool, SqliteWriteGate::default(), 100_000);

    assert!(store.get_many(&[entry(1).cache_key], 4).await.is_err());
}

#[tokio::test]
async fn missing_state_singleton_self_heals_instead_of_failing_writes() {
    let (store, pool, _) = store().await;
    store
        .put_many(&(0..5).map(entry).collect::<Vec<_>>(), 100)
        .await
        .expect("seed write");
    // Simulate external drift: the O(1) cardinality row disappears. Triggers
    // fire only on cache-table changes, so nothing would ever restore it.
    sqlx::query("DELETE FROM embedding_vector_cache_state")
        .execute(&pool)
        .await
        .expect("drop state singleton");

    store
        .put_many(&(5..8).map(entry).collect::<Vec<_>>(), 100)
        .await
        .expect("write after drift must self-heal, not error");
    let count: i64 = sqlx::query_scalar(
        "SELECT entry_count FROM embedding_vector_cache_state WHERE singleton = 1",
    )
    .fetch_one(&pool)
    .await
    .expect("recomputed singleton");
    assert_eq!(count, 8);
}
