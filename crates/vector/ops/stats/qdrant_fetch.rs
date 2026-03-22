use crate::crates::core::config::Config;
use crate::crates::vector::ops::qdrant::qdrant_base;
use std::error::Error;

pub(super) async fn fetch_qdrant_snapshots(
    cfg: &Config,
    client: &reqwest::Client,
) -> Result<(serde_json::Value, serde_json::Value, serde_json::Value), Box<dyn Error>> {
    let base = qdrant_base(cfg);
    let col = &cfg.collection;

    // All three requests are independent — run concurrently with tokio::join!
    // to eliminate serial round-trip latency (saves 10-30ms per stats call).
    let (info_res, count_res, docs_count_res) = tokio::join!(
        async {
            client
                .get(format!("{base}/collections/{col}"))
                .send()
                .await?
                .error_for_status()?
                .json::<serde_json::Value>()
                .await
        },
        async {
            client
                .post(format!("{base}/collections/{col}/points/count"))
                .json(&serde_json::json!({"exact": false}))
                .send()
                .await?
                .error_for_status()?
                .json::<serde_json::Value>()
                .await
        },
        async {
            client
                .post(format!("{base}/collections/{col}/points/count"))
                .json(&serde_json::json!({
                    "exact": false,
                    "filter": {"must": [{"key": "chunk_index", "match": { "value": 0 }}]}
                }))
                .send()
                .await?
                .error_for_status()?
                .json::<serde_json::Value>()
                .await
        },
    );

    Ok((info_res?, count_res?, docs_count_res?))
}
