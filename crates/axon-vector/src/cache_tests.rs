//! Tests for the document-chunk cache.

use super::doc_cache::{DocCache, DocCacheConfig, DocCacheKey, doc_cache_for_config};
use super::enforce_core_dump_disabled_for_ask_cache;
use super::generation::{bump_generation, current_generation};
use crate::ops::qdrant::{QdrantPayload, QdrantPoint};
use axon_core::config::Config;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn make_point(text: &str) -> QdrantPoint {
    QdrantPoint {
        id: serde_json::json!(0),
        payload: QdrantPayload {
            url: "http://example.com/foo".into(),
            chunk_text: text.into(),
            text: String::new(),
            chunk_index: Some(0),
            ..QdrantPayload::default()
        },
    }
}

fn key(collection: &str, url: &str, generation: u64) -> DocCacheKey {
    DocCacheKey {
        collection: collection.into(),
        url: url.into(),
        generation,
    }
}

#[tokio::test]
async fn cache_hit_returns_same_arc() {
    let cache = DocCache::new(DocCacheConfig::default());
    let k = key("c1", "http://example.com/a", 0);

    let first = cache
        .get_or_fetch(k.clone(), || async { Ok(vec![make_point("hello")]) })
        .await
        .expect("first fetch ok");

    let second = cache
        .get_or_fetch(k, || async {
            panic!("must not call fetch on cache hit");
        })
        .await
        .expect("second fetch ok");

    assert!(Arc::ptr_eq(&first, &second), "cache must return same Arc");
}

#[tokio::test]
async fn cache_miss_falls_through_to_provider() {
    let cache = DocCache::new(DocCacheConfig::default());
    let counter = Arc::new(AtomicUsize::new(0));

    let counter_clone = Arc::clone(&counter);
    let _ = cache
        .get_or_fetch(key("c1", "http://example.com/b", 0), || {
            let c = Arc::clone(&counter_clone);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(vec![make_point("hi")])
            }
        })
        .await
        .unwrap();

    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn concurrent_misses_share_one_fetch() {
    let cache = Arc::new(DocCache::new(DocCacheConfig::default()));
    let counter = Arc::new(AtomicUsize::new(0));
    let k = key("c1", "http://example.com/c", 0);

    let mut joins = Vec::new();
    for _ in 0..10 {
        let cache = Arc::clone(&cache);
        let counter = Arc::clone(&counter);
        let k = k.clone();
        joins.push(tokio::spawn(async move {
            cache
                .get_or_fetch(k, || {
                    let c = Arc::clone(&counter);
                    async move {
                        // Widen the race window so concurrent waiters all queue
                        // up before the first fetcher returns.
                        c.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        Ok(vec![make_point("shared")])
                    }
                })
                .await
                .unwrap()
        }));
    }
    for j in joins {
        j.await.unwrap();
    }
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "single-flight: exactly one fetch for N concurrent callers"
    );
}

#[tokio::test]
async fn ttl_expiry_invalidates() {
    let cfg = DocCacheConfig {
        max_capacity_bytes: 1024 * 1024,
        ttl_secs: 1, // hard-cap floor; we sleep through it
    };
    // moka enforces a minimum granularity — use a real sleep, not paused time,
    // because moka reads std::time::Instant internally.
    let cache = DocCache::new(cfg);
    let k = key("c1", "http://example.com/d", 0);
    let counter = Arc::new(AtomicUsize::new(0));

    let counter_clone = Arc::clone(&counter);
    cache
        .get_or_fetch(k.clone(), || {
            let c = Arc::clone(&counter_clone);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(vec![make_point("ttl")])
            }
        })
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(1100)).await;
    cache.run_pending_tasks().await;

    let counter_clone = Arc::clone(&counter);
    cache
        .get_or_fetch(k, || {
            let c = Arc::clone(&counter_clone);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(vec![make_point("ttl-2")])
            }
        })
        .await
        .unwrap();

    assert_eq!(
        counter.load(Ordering::SeqCst),
        2,
        "post-TTL fetch must miss and refetch"
    );
}

#[tokio::test]
async fn generation_bump_makes_new_key_miss() {
    let cache = DocCache::new(DocCacheConfig::default());
    let unique = format!("gen-test-{}", uuid::Uuid::new_v4());
    let url = "http://example.com/gen";

    let g0 = current_generation(&unique);
    let counter = Arc::new(AtomicUsize::new(0));

    let counter_clone = Arc::clone(&counter);
    cache
        .get_or_fetch(key(&unique, url, g0), || {
            let c = Arc::clone(&counter_clone);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(vec![make_point("v0")])
            }
        })
        .await
        .unwrap();

    bump_generation(&unique);
    let g1 = current_generation(&unique);
    assert_ne!(g0, g1, "bump must change generation");

    let counter_clone = Arc::clone(&counter);
    cache
        .get_or_fetch(key(&unique, url, g1), || {
            let c = Arc::clone(&counter_clone);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(vec![make_point("v1")])
            }
        })
        .await
        .unwrap();

    assert_eq!(
        counter.load(Ordering::SeqCst),
        2,
        "post-bump key must miss and refetch"
    );
}

#[tokio::test]
async fn byte_weigher_evicts_under_capacity_pressure() {
    // Tiny capacity (4 KiB) forces eviction once a few entries are stored.
    let cfg = DocCacheConfig {
        max_capacity_bytes: 4 * 1024,
        ttl_secs: 60,
    };
    let cache = DocCache::new(cfg);

    // Each entry is ~2 KiB of chunk_text.
    let body: String = "x".repeat(2 * 1024);
    for i in 0..10 {
        let url = format!("http://example.com/big/{i}");
        cache
            .get_or_fetch(key("c1", &url, 0), || async { Ok(vec![make_point(&body)]) })
            .await
            .unwrap();
    }

    cache.run_pending_tasks().await;
    let stats = cache.stats();
    assert!(
        stats.evicted() > 0,
        "byte-weigher must trigger evictions under capacity pressure"
    );
}

#[test]
fn process_cache_registry_is_keyed_by_configured_capacity_and_ttl() {
    let a = doc_cache_for_config(DocCacheConfig {
        max_capacity_bytes: 4096,
        ttl_secs: 1,
    });
    let b = doc_cache_for_config(DocCacheConfig {
        max_capacity_bytes: 8192,
        ttl_secs: 2,
    });
    let a_again = doc_cache_for_config(DocCacheConfig {
        max_capacity_bytes: 4096,
        ttl_secs: 1,
    });

    assert!(Arc::ptr_eq(&a, &a_again), "same config must reuse cache");
    assert!(
        !Arc::ptr_eq(&a, &b),
        "different configured capacity/TTL must use a different cache"
    );
    assert_eq!(a.config().max_capacity_bytes, 4096);
    assert_eq!(a.config().effective_ttl_secs(), 1);
    assert_eq!(b.config().max_capacity_bytes, 8192);
    assert_eq!(b.config().effective_ttl_secs(), 2);
}

#[test]
fn core_dump_guard_is_noop_when_cache_disabled() {
    let cfg = Config {
        ask_cache_enabled: false,
        ..Config::default()
    };

    enforce_core_dump_disabled_for_ask_cache(&cfg).expect("disabled cache must not alter limits");
}

#[tokio::test]
async fn invalidate_collection_drops_all_keys() {
    let cache = DocCache::new(DocCacheConfig::default());
    cache
        .get_or_fetch(key("col-a", "http://x/1", 0), || async {
            Ok(vec![make_point("a")])
        })
        .await
        .unwrap();
    cache
        .get_or_fetch(key("col-a", "http://x/2", 0), || async {
            Ok(vec![make_point("b")])
        })
        .await
        .unwrap();
    cache
        .get_or_fetch(key("col-b", "http://x/3", 0), || async {
            Ok(vec![make_point("c")])
        })
        .await
        .unwrap();

    cache.invalidate_collection("col-a");
    cache.run_pending_tasks().await;

    // col-a entries must miss; col-b must still hit.
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_c = Arc::clone(&counter);
    cache
        .get_or_fetch(key("col-a", "http://x/1", 0), || {
            let c = Arc::clone(&counter_c);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(vec![make_point("a-2")])
            }
        })
        .await
        .unwrap();
    cache
        .get_or_fetch(key("col-b", "http://x/3", 0), || {
            let c = Arc::clone(&counter);
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(vec![make_point("c-2")])
            }
        })
        .await
        .unwrap();

    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "only the invalidated collection should refetch"
    );
}
