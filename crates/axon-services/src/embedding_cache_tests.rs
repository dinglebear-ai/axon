use std::sync::Arc;

use axon_api::source::{
    BatchId, ChunkId, ContentKind, EmbeddingBatch, EmbeddingInput, JobId, JobPriority, MetadataMap,
    ProviderId,
};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::migrations::apply_all_migrations;
use sqlx::sqlite::SqlitePoolOptions;

use super::*;

async fn pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect sqlite");
    apply_all_migrations(&pool).await.expect("apply migrations");
    pool
}

fn batch(texts: &[&str]) -> EmbeddingBatch {
    EmbeddingBatch {
        batch_id: BatchId::new(uuid::Uuid::from_u128(1)),
        job_id: JobId::new(uuid::Uuid::from_u128(2)),
        provider_id: ProviderId::new("tei"),
        model: "fake-embedding".to_string(),
        items: texts
            .iter()
            .enumerate()
            .map(|(index, text)| EmbeddingInput {
                chunk_id: ChunkId::new(format!("chunk-{index}")),
                text: (*text).to_string(),
                content_kind: ContentKind::Markdown,
                metadata: MetadataMap::new(),
            })
            .collect(),
        instruction: Some("document".to_string()),
        priority: JobPriority::Normal,
        metadata: MetadataMap::new(),
    }
}

fn provider(pool: SqlitePool, fake: Arc<FakeEmbeddingProvider>) -> CachedEmbeddingProvider {
    CachedEmbeddingProvider::new(
        fake,
        pool,
        "http://tei.test",
        ProviderId::new("tei"),
        "fake-embedding",
        4,
        100_000,
    )
}

#[tokio::test]
async fn repeated_inputs_are_served_from_cache_in_original_order() {
    let pool = pool().await;
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let cached = provider(pool, Arc::clone(&fake));

    let first = cached
        .embed(batch(&["alpha", "beta", "alpha"]))
        .await
        .unwrap();
    let cached_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embedding_vector_cache")
        .fetch_one(&cached.pool)
        .await
        .unwrap();
    assert_eq!(cached_count, 2);
    let second = cached.embed(batch(&["beta", "alpha"])).await.unwrap();

    let calls = fake.calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].items.len(), 2);
    assert_eq!(first.vectors.len(), 3);
    assert_eq!(first.vectors[0].values, first.vectors[2].values);
    assert_eq!(second.vectors[0].values, first.vectors[1].values);
    assert_eq!(second.vectors[1].values, first.vectors[0].values);
    assert_eq!(second.vectors[0].chunk_id.0, "chunk-0");
    assert_eq!(second.usage.requests, 0);
}

#[tokio::test]
async fn key_separates_authority_model_instruction_kind_and_text() {
    let base = batch(&["same"]);
    let input = &base.items[0];
    let provider = ProviderId::new("tei");
    let original = cache_key("authority-a", &provider, "model-a", 4, &base, input);

    let mut changed_instruction = base.clone();
    changed_instruction.instruction = Some("query".to_string());
    let mut changed_kind = input.clone();
    changed_kind.content_kind = ContentKind::Code;
    let mut changed_text = input.clone();
    changed_text.text = "different".to_string();

    assert_ne!(
        original,
        cache_key("authority-b", &provider, "model-a", 4, &base, input)
    );
    assert_ne!(
        original,
        cache_key("authority-a", &provider, "model-b", 4, &base, input)
    );
    assert_ne!(
        original,
        cache_key("authority-a", &provider, "model-a", 8, &base, input)
    );
    assert_ne!(
        original,
        cache_key(
            "authority-a",
            &provider,
            "model-a",
            4,
            &changed_instruction,
            input
        )
    );
    assert_ne!(
        original,
        cache_key("authority-a", &provider, "model-a", 4, &base, &changed_kind)
    );
    assert_ne!(
        original,
        cache_key("authority-a", &provider, "model-a", 4, &base, &changed_text)
    );
}

#[tokio::test]
async fn corrupt_cache_rows_fail_open_to_provider() {
    let pool = pool().await;
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let cached = provider(pool.clone(), Arc::clone(&fake));
    let request = batch(&["alpha"]);
    let key = cache_key(
        "http://tei.test",
        &ProviderId::new("tei"),
        "fake-embedding",
        4,
        &request,
        &request.items[0],
    );
    sqlx::query(
        "INSERT INTO embedding_vector_cache \
         (cache_key, provider_id, model, dimensions, vector, created_at, last_used_at) \
         VALUES (?, 'tei', 'fake-embedding', 4, X'00', 0, 0)",
    )
    .bind(key)
    .execute(&pool)
    .await
    .unwrap();

    let result = cached.embed(request).await.unwrap();
    assert_eq!(result.vectors.len(), 1);
    assert_eq!(fake.calls().await.len(), 1);
}

#[tokio::test]
async fn retention_prunes_oldest_entries_deterministically() {
    let pool = pool().await;
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let cached = CachedEmbeddingProvider::new(
        fake,
        pool.clone(),
        "http://tei.test",
        ProviderId::new("tei"),
        "fake-embedding",
        4,
        2,
    );

    cached
        .embed(batch(&["alpha", "beta", "gamma"]))
        .await
        .unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embedding_vector_cache")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn production_sized_batch_is_persisted_by_one_bulk_write() {
    let pool = pool().await;
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let cached = provider(pool.clone(), Arc::clone(&fake));
    let texts = (0..512)
        .map(|index| format!("text-{index}"))
        .collect::<Vec<_>>();
    let text_refs = texts.iter().map(String::as_str).collect::<Vec<_>>();

    cached.embed(batch(&text_refs)).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embedding_vector_cache")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 512);

    cached.embed(batch(&text_refs)).await.unwrap();
    assert_eq!(fake.calls().await.len(), 1);
}
