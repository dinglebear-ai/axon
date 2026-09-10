use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Default)]
struct ReplenishmentFetch {
    slow_done: Arc<AtomicBool>,
    third_started_before_slow_done: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl FetchProvider for ReplenishmentFetch {
    async fn fetch(
        &self,
        request: axon_api::source::FetchRequest,
    ) -> crate::boundary::Result<axon_api::source::FetchedResource> {
        if request.uri.ends_with("/slow") {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            self.slow_done.store(true, Ordering::Release);
        } else if request.uri.ends_with("/fast") {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        } else if request.uri.ends_with("/third") {
            self.third_started_before_slow_done
                .store(!self.slow_done.load(Ordering::Acquire), Ordering::Release);
        }
        FetchProvider::fetch(
            &crate::boundary::FakeAdapterProviders::new()
                .with_fetch_text("<html><body>backfill content</body></html>"),
            request,
        )
        .await
    }

    async fn capabilities(&self) -> crate::boundary::Result<axon_api::source::ProviderCapability> {
        FetchProvider::capabilities(&crate::boundary::FakeAdapterProviders::new()).await
    }
}

#[tokio::test]
async fn backfill_replenishes_capacity_before_slowest_fetch_finishes() {
    let fetch = ReplenishmentFetch::default();
    let observation = fetch.third_started_before_slow_done.clone();
    let output = tempfile::tempdir().expect("output");
    let cfg = Config {
        backfill_concurrency_limit: Some(2),
        batch_concurrency: 2,
        min_markdown_chars: 0,
        drop_thin_markdown: false,
        ..Config::default()
    };
    let mut summary = CrawlSummary::default();

    let (_, added) = append_candidate_backfill(
        &cfg,
        Arc::new(fetch),
        output.path(),
        &HashSet::new(),
        vec![
            "https://example.com/slow".to_string(),
            "https://example.com/fast".to_string(),
            "https://example.com/third".to_string(),
        ],
        &mut summary,
    )
    .await
    .expect("backfill");

    assert!(observation.load(Ordering::Acquire));
    let expected = vec![
        "https://example.com/slow".to_string(),
        "https://example.com/fast".to_string(),
        "https://example.com/third".to_string(),
    ];
    assert_eq!(added, expected);
    let manifest = tokio::fs::read_to_string(output.path().join("manifest.jsonl"))
        .await
        .expect("manifest");
    let urls = manifest
        .lines()
        .map(|line| {
            serde_json::from_str::<ManifestEntry>(line)
                .expect("entry")
                .url
        })
        .collect::<Vec<_>>();
    assert_eq!(urls, expected);
}
