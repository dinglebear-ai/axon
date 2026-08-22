use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::task::Poll;

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
        SqliteEmbeddingVectorCacheStore::new(pool.clone(), gate.clone()),
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
    assert!(lookup.corrupt_keys.is_empty());
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
    assert_eq!(lookup.corrupt_keys, vec![key.clone()]);

    store.retire_many(std::slice::from_ref(&key)).await.unwrap();
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
    assert_eq!(lookup.corrupt_keys, vec![nan_key, infinite_key]);
}

#[tokio::test]
async fn retention_prunes_deterministically_after_chunked_writes() {
    let (store, pool, _) = store().await;
    let entries = (0..1_000).map(entry).collect::<Vec<_>>();

    store.put_many(&entries, 250).await.unwrap();

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
    let store = SqliteEmbeddingVectorCacheStore::new(pool.clone(), gate.clone());
    let mut only_connection = pool.acquire().await.expect("only pool connection");
    let held_gate = gate.lock().await;

    let scheduler = ProviderScheduler::new_with_write_gate(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "tei".into(),
            authority_id: "test".into(),
        },
        SchedulerConfig {
            capacity: 1,
            interactive_reserve: 0,
            max_entries: 16,
            max_units: 16,
        },
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
    let store = SqliteEmbeddingVectorCacheStore::new(pool, SqliteWriteGate::default());

    assert!(store.get_many(&[entry(1).cache_key], 4).await.is_err());
}
