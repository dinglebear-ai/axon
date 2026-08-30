use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use std::{future::Future, future::poll_fn, task::Poll};

use axon_api::source::{
    BatchId, ChunkId, ContentKind, EmbeddingBatch, EmbeddingInput, JobId, JobPriority, MetadataMap,
    ProviderId,
};
use axon_core::config::Config;
use axon_embedding::cache::{CachedEmbedding, EmbeddingVectorCacheStore};
use axon_embedding::provider::EmbeddingProvider;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_jobs::embedding_cache_store::SqliteEmbeddingVectorCacheStore;
use axon_jobs::scheduler::SqliteWriteGate;
use httpmock::MockServer;
use sqlx::sqlite::SqlitePoolOptions;
use tokio::sync::Barrier;

use super::{
    RuntimeSchedulers, artifact_candidate_sink_from_values, build_runtime_schedulers,
    invalidate_embedding_identity_cache, resolve_embedding_identity, tei_max_attempts,
};
use crate::context::TargetLocalSourceRuntime;

async fn assert_pending<F: Future>(mut future: Pin<&mut F>, message: &str) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending(), "{message}");
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn artifact_candidate_sink_configuration_is_all_or_nothing() {
    let disabled = artifact_candidate_sink_from_values(None, None).expect("disabled sink");
    assert_eq!(disabled.capabilities().await.unwrap().name, "noop");

    assert!(
        artifact_candidate_sink_from_values(Some("https://depot.example".to_string()), None,)
            .is_err()
    );
    assert!(artifact_candidate_sink_from_values(None, Some("write-token".to_string())).is_err());
    for (url, token) in [
        (Some(String::new()), Some(String::new())),
        (Some("   ".to_string()), Some("write-token".to_string())),
        (
            Some("https://depot.example".to_string()),
            Some("   ".to_string()),
        ),
    ] {
        assert!(
            artifact_candidate_sink_from_values(url, token).is_err(),
            "present but blank Depot configuration must fail construction"
        );
    }
    assert!(
        artifact_candidate_sink_from_values(
            Some("not a URL".to_string()),
            Some("write-token".to_string()),
        )
        .is_err()
    );

    let configured = artifact_candidate_sink_from_values(
        Some("https://depot.example".to_string()),
        Some("write-token".to_string()),
    )
    .expect("valid Depot sink");
    assert_eq!(configured.capabilities().await.unwrap().name, "depot-http");
}

#[tokio::test]
async fn invalid_explicit_depot_configuration_fails_runtime_construction() {
    for (url, token) in [
        (Some("not a URL".to_string()), Some("token".to_string())),
        (Some("https://depot.example".to_string()), None),
        (None, Some("token".to_string())),
    ] {
        assert!(artifact_candidate_sink_from_values(url, token).is_err());
    }
}

/// `tei_max_attempts` is the one place `cfg.tei_max_retries` becomes the real
/// attempt budget threaded into `TeiEmbeddingConfig::max_attempts` — previously
/// `TeiEmbeddingProvider` always used a hardcoded `MAX_ATTEMPTS = 6` regardless
/// of `[providers.embedding].max-retries`/`TEI_MAX_RETRIES`.
#[test]
fn tei_max_attempts_reflects_configured_retry_count_not_a_hardcoded_default() {
    let mut cfg = Config::test_default();

    cfg.tei_max_retries = 5;
    assert_eq!(
        tei_max_attempts(&cfg),
        6,
        "default tei_max_retries=5 should still yield the historical 6 total attempts"
    );

    cfg.tei_max_retries = 2;
    assert_eq!(
        tei_max_attempts(&cfg),
        3,
        "a non-default tei_max_retries must change the computed attempt budget"
    );

    cfg.tei_max_retries = 0;
    assert_eq!(
        tei_max_attempts(&cfg),
        1,
        "zero retries still allows exactly one (the initial) attempt"
    );
}

/// `from_config` builds the three real stores + reservations from the shared
/// SQLite pool (one runtime DB) and a dummy Qdrant URL. The ledger binds to the
/// caller-supplied pool via `from_pool` without running its own migrations (the
/// tables are owned by the shared migration runner), and the Qdrant constructor
/// does not connect.
///
/// The embedding identity is now derived from the live TEI `/info` + a probe
/// embed. To keep this unit test hermetic and deterministic, `tei_url` points at
/// a closed loopback port so the derivation always fails fast and falls back to
/// the configured defaults — proving the fallback path stamps the model/dims.
#[tokio::test]
async fn source_db_stage_capacity_reserves_one_control_connection() {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .expect("four-connection pool");
    assert_eq!(super::source_db_stage_capacity(&pool), 3);

    let single = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("single-connection pool");
    assert_eq!(
        super::source_db_stage_capacity(&single),
        1,
        "single-connection test pools must retain one usable data-plane slot"
    );
}

#[tokio::test]
async fn production_schedulers_share_one_gate_before_pool_acquisition() {
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect("sqlite::memory:")
        .await
        .expect("eight-connection pool");
    axon_jobs::migrations::apply_all_migrations(&pool)
        .await
        .expect("shared runtime migrations");

    let cfg = Config::test_default();
    let RuntimeSchedulers {
        embedding,
        vector,
        fetch,
        render,
        parse,
        graph,
        artifact,
    } = build_runtime_schedulers(
        &cfg,
        &pool,
        &ProviderId::new("test-embedding"),
        &ProviderId::new("test-vector"),
        SqliteWriteGate::default(),
    )
    .await
    .expect("production schedulers");

    // Ensure every pool acquisition below is immediately ready. With one
    // writer connection held, the first reconciliation consumes one more
    // connection while waiting in SQLite. The other six production schedulers
    // must stop at the same process-local gate and leave the remaining six
    // connections available to the control plane.
    let mut warmed = Vec::new();
    for _ in 0..pool.options().get_max_connections() {
        warmed.push(pool.acquire().await.expect("pre-warm pool connection"));
    }
    drop(warmed);
    let held = axon_core::sqlite::ImmediateTx::begin(&pool)
        .await
        .expect("hold writer lock");

    let schedulers = [embedding, vector, fetch, render, parse, graph, artifact];
    let mut waiters = schedulers
        .iter()
        .map(|scheduler| Box::pin(scheduler.reconcile()))
        .collect::<Vec<_>>();
    for waiter in &mut waiters {
        poll_fn(|cx| {
            assert!(
                waiter.as_mut().poll(cx).is_pending(),
                "reconciliation must wait behind the held SQLite writer"
            );
            Poll::Ready(())
        })
        .await;
    }

    let control_connection = pool
        .try_acquire()
        .expect("production scheduler contention must preserve a control-plane pool slot");
    drop(control_connection);
    held.rollback().await;
    for waiter in waiters {
        waiter.await.expect("reconcile after writer release");
    }
}

#[tokio::test]
async fn production_runtime_cache_and_schedulers_share_one_gate_before_pool_acquisition() {
    let server = MockServer::start_async().await;
    let _info = server
        .mock_async(|when, then| {
            when.method("GET").path("/info");
            then.status(200)
                .json_body(serde_json::json!({ "model_id": "acme/shared-gate" }));
        })
        .await;
    let _embed = server
        .mock_async(|when, then| {
            when.method("POST").path("/embed");
            then.status(200)
                .json_body(serde_json::json!([[0.1_f32, 0.2_f32, 0.3_f32]]));
        })
        .await;
    let mut cfg = Config::test_default();
    cfg.tei_url = server.base_url();
    cfg.tei_request_timeout_ms = 1_000;
    cfg.embed_cache_enabled = true;
    invalidate_embedding_identity_cache(&cfg);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect("sqlite::memory:")
        .await
        .expect("eight-connection pool");
    axon_jobs::migrations::apply_all_migrations(&pool)
        .await
        .expect("shared runtime migrations");
    let jobs: Arc<dyn JobStore> = Arc::new(FakeJobWatchStore::new());
    let runtime = TargetLocalSourceRuntime::from_config(&cfg, jobs, pool.clone())
        .await
        .expect("production runtime");
    let cache_store: Arc<SqliteEmbeddingVectorCacheStore> = runtime
        .embedding_cache_store
        .clone()
        .expect("enabled cache store");
    let scheduler = runtime
        .embedding_scheduler
        .as_ref()
        .expect("production embedding scheduler");

    let mut warmed = Vec::new();
    for _ in 0..pool.options().get_max_connections() {
        warmed.push(pool.acquire().await.expect("pre-warm pool connection"));
    }
    drop(warmed);
    // SQLx returns a dropped `PoolConnection` to the idle set from a spawned
    // task. Spinning on `yield_now` only reschedules this runtime's ready
    // queue, so under a loaded `cargo test` (every other test racing for the
    // same cores) the return tasks may not have run yet and the assertion
    // below sees a partially refilled pool. Wait on wall-clock time instead.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while pool.num_idle() < pool.options().get_max_connections() as usize
        && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let idle_before = pool.num_idle();
    assert!(
        idle_before > 0,
        "the pre-warmed pool must expose an idle connection before polling writers"
    );
    let held_gate = runtime.sqlite_write_gate.lock().await;
    let entries = [CachedEmbedding {
        cache_key: format!("sha256:{}", "1".repeat(64)),
        provider_id: ProviderId::new("tei"),
        model: runtime.embedding_model.clone(),
        dimensions: runtime.embedding_dimensions,
        values: vec![0.5; runtime.embedding_dimensions as usize],
    }];
    let mut cache_write = Box::pin(cache_store.put_many(&entries, 100));
    let mut scheduler_write = Box::pin(scheduler.reconcile());
    assert_pending(
        cache_write.as_mut(),
        "production cache writer must wait on the composed gate",
    )
    .await;
    assert_pending(
        scheduler_write.as_mut(),
        "production scheduler writer must wait on the composed gate",
    )
    .await;
    assert_eq!(
        pool.num_idle(),
        idle_before,
        "production gate waiters must not acquire pool connections"
    );
    drop(
        pool.try_acquire()
            .expect("production gate waiters must preserve a control-plane connection"),
    );
    drop(held_gate);
    cache_write.await.expect("cache write");
    scheduler_write.await.expect("scheduler write");
}

#[tokio::test]
async fn from_config_falls_back_to_default_embedding_identity_when_tei_unreachable() {
    let mut cfg = Config::test_default();
    cfg.qdrant_url = "http://127.0.0.1:53333".to_string();
    // Closed port → derivation fails fast → fallback identity.
    cfg.tei_url = "http://127.0.0.1:1".to_string();
    cfg.tei_request_timeout_ms = 250;
    cfg.embed_prep_concurrency = 3;
    cfg.embed_pool_max_inputs = 640;
    // Cache enabled in config, but the identity below is unverified fallback —
    // the runtime must fail open to the raw provider with no cache decoration.
    cfg.embed_cache_enabled = true;

    let jobs: Arc<dyn JobStore> = Arc::new(FakeJobWatchStore::new());
    // The ledger binds to this shared pool (no separate ledger.db, no eager I/O).
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite pool");
    axon_jobs::migrations::apply_all_migrations(&pool)
        .await
        .expect("shared runtime migrations");

    let runtime = TargetLocalSourceRuntime::from_config(&cfg, jobs, pool.clone())
        .await
        .expect("build target local-source runtime");

    assert_eq!(runtime.embedding_provider_id.0, "target-local-embed");
    assert_eq!(runtime.vector_provider_id.0, "target-local-vector");
    // Fallback identity when the live provider cannot be reached.
    assert_eq!(runtime.embedding_model, "Qwen3-Embedding-0.6B");
    assert_eq!(runtime.embedding_dimensions, 1024);
    assert_eq!(runtime.document_prepare_concurrency, 3);
    assert_eq!(runtime.embed_pool_max_inputs, 640);
    let persisted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM provider_identity_cache")
        .fetch_one(&pool)
        .await
        .expect("count durable identities");
    assert_eq!(
        persisted, 0,
        "unverified fallback identity must never poison the durable cache"
    );
    assert!(
        runtime.embedding_cache_store.is_none(),
        "an unverified embedding identity must not enable the embedding vector cache"
    );
}

#[tokio::test]
async fn runtime_embedding_cache_respects_enabled_and_disabled_configuration() {
    for (cache_enabled, expected_embed_calls, expected_cache_rows) in
        [(false, 3, 0_i64), (true, 2, 1_i64)]
    {
        let server = MockServer::start_async().await;
        let _info = server
            .mock_async(|when, then| {
                when.method("GET").path("/info");
                then.status(200)
                    .json_body(serde_json::json!({ "model_id": "acme/runtime-cache" }));
            })
            .await;
        let embed = server
            .mock_async(|when, then| {
                when.method("POST").path("/embed");
                then.status(200)
                    .json_body(serde_json::json!([[0.1_f32, 0.2_f32, 0.3_f32]]));
            })
            .await;
        let mut cfg = Config::test_default();
        cfg.qdrant_url = "http://127.0.0.1:53333".into();
        cfg.tei_url = server.base_url();
        cfg.tei_request_timeout_ms = 1_000;
        cfg.embed_cache_enabled = cache_enabled;
        invalidate_embedding_identity_cache(&cfg);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        axon_jobs::migrations::apply_all_migrations(&pool)
            .await
            .unwrap();
        let jobs: Arc<dyn JobStore> = Arc::new(FakeJobWatchStore::new());
        let runtime = TargetLocalSourceRuntime::from_config(&cfg, jobs, pool.clone())
            .await
            .unwrap();
        assert_eq!(
            runtime.embedding_cache_store.is_some(),
            cache_enabled,
            "a verified identity must decorate with the cache exactly when enabled"
        );
        let request = EmbeddingBatch {
            batch_id: BatchId::new(uuid::Uuid::new_v4()),
            job_id: JobId::new(uuid::Uuid::new_v4()),
            provider_id: ProviderId::new("tei"),
            model: runtime.embedding_model.clone(),
            items: vec![EmbeddingInput {
                chunk_id: ChunkId::new("runtime-cache"),
                text: "repeat me".into(),
                content_kind: ContentKind::Markdown,
                metadata: MetadataMap::new(),
            }],
            instruction: None,
            priority: JobPriority::Normal,
            metadata: MetadataMap::new(),
        };

        runtime
            .embedding_provider
            .embed(request.clone())
            .await
            .unwrap();
        runtime.embedding_provider.embed(request).await.unwrap();

        embed.assert_calls_async(expected_embed_calls).await;
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embedding_vector_cache")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, expected_cache_rows);
    }
}

#[tokio::test]
async fn embedding_identity_cache_singleflights_cold_probes_and_can_be_invalidated() {
    let server = MockServer::start_async().await;
    let info = server
        .mock_async(|when, then| {
            when.method("GET").path("/info");
            then.status(200)
                .delay(Duration::from_millis(100))
                .json_body(serde_json::json!({ "model_id": "acme/embedding" }));
        })
        .await;
    let embed = server
        .mock_async(|when, then| {
            when.method("POST").path("/embed");
            then.status(200)
                .json_body(serde_json::json!([[0.1_f32, 0.2_f32, 0.3_f32]]));
        })
        .await;
    let mut cfg = Config::test_default();
    cfg.tei_url = server.base_url();
    cfg.tei_request_timeout_ms = 1_000;
    invalidate_embedding_identity_cache(&cfg);

    let callers = 8;
    let barrier = Arc::new(Barrier::new(callers + 1));
    let mut tasks = Vec::with_capacity(callers);
    for _ in 0..callers {
        let barrier = Arc::clone(&barrier);
        let cfg = cfg.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            resolve_embedding_identity(&cfg).await
        }));
    }
    barrier.wait().await;
    for task in tasks {
        let identity = task.await.expect("identity task");
        assert_eq!(identity.model, "acme/embedding");
        assert_eq!(identity.dimensions, 3);
    }
    info.assert_calls_async(1).await;
    embed.assert_calls_async(1).await;

    // A warm read is cache-only. Explicit invalidation makes the next caller
    // reprobe immediately instead of waiting for the 30-second positive TTL.
    let warm = resolve_embedding_identity(&cfg).await;
    assert_eq!(warm.dimensions, 3);
    info.assert_calls_async(1).await;
    invalidate_embedding_identity_cache(&cfg);
    let refreshed = resolve_embedding_identity(&cfg).await;
    assert_eq!(refreshed.dimensions, 3);
    info.assert_calls_async(2).await;
    embed.assert_calls_async(2).await;
}

#[tokio::test]
async fn from_config_reuses_verified_embedding_identity_after_process_cache_invalidation() {
    let server = MockServer::start_async().await;
    let info = server
        .mock_async(|when, then| {
            when.method("GET").path("/info");
            then.status(200)
                .json_body(serde_json::json!({ "model_id": "acme/durable-embedding" }));
        })
        .await;
    let embed = server
        .mock_async(|when, then| {
            when.method("POST").path("/embed");
            then.status(200)
                .json_body(serde_json::json!([[0.1_f32, 0.2_f32, 0.3_f32]]));
        })
        .await;
    let mut cfg = Config::test_default();
    cfg.qdrant_url = "http://127.0.0.1:53333".to_string();
    cfg.tei_url = server.base_url();
    cfg.tei_request_timeout_ms = 1_000;
    invalidate_embedding_identity_cache(&cfg);

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .expect("runtime pool");
    axon_jobs::migrations::apply_all_migrations(&pool)
        .await
        .expect("shared runtime migrations");
    let jobs: Arc<dyn JobStore> = Arc::new(FakeJobWatchStore::new());

    let first = TargetLocalSourceRuntime::from_config(&cfg, Arc::clone(&jobs), pool.clone())
        .await
        .expect("first runtime");
    assert_eq!(first.embedding_model, "acme/durable-embedding");
    assert_eq!(first.embedding_dimensions, 3);
    info.assert_calls_async(1).await;
    embed.assert_calls_async(1).await;

    // Simulate a new short-lived CLI process: the process cache is gone, but
    // the unified SQLite runtime survives. The second runtime must not probe
    // TEI again.
    invalidate_embedding_identity_cache(&cfg);
    let second = TargetLocalSourceRuntime::from_config(&cfg, jobs, pool.clone())
        .await
        .expect("second runtime");
    assert_eq!(second.embedding_model, "acme/durable-embedding");
    assert_eq!(second.embedding_dimensions, 3);
    info.assert_calls_async(1).await;
    embed.assert_calls_async(1).await;

    let persisted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM provider_identity_cache")
        .fetch_one(&pool)
        .await
        .expect("count durable identity");
    assert_eq!(persisted, 1);
}

#[test]
fn scheduler_authority_canonicalizes_equivalent_sqlite_paths() {
    let dir = tempfile::tempdir().expect("tempdir");
    let nested = dir.path().join("nested");
    std::fs::create_dir_all(&nested).expect("nested dir");
    let database = dir.path().join("jobs.db");
    std::fs::write(&database, []).expect("database placeholder");
    let equivalent = nested.join("..").join("jobs.db");

    assert_eq!(
        super::scheduler_authority_id(&database),
        super::scheduler_authority_id(&equivalent)
    );
}
