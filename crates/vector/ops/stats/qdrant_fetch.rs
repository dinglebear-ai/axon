use crate::crates::core::config::Config;
use crate::crates::vector::ops::qdrant::qdrant_base;
use std::error::Error;

pub(super) async fn fetch_qdrant_snapshots(
    cfg: &Config,
    client: &reqwest::Client,
) -> Result<(serde_json::Value, serde_json::Value, serde_json::Value), Box<dyn Error>> {
    let info = client
        .get(format!(
            "{}/collections/{}",
            qdrant_base(cfg),
            cfg.collection
        ))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    let count = client
        .post(format!(
            "{}/collections/{}/points/count",
            qdrant_base(cfg),
            cfg.collection
        ))
        .json(&serde_json::json!({"exact": false}))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    let docs_count = client
        .post(format!(
            "{}/collections/{}/points/count",
            qdrant_base(cfg),
            cfg.collection
        ))
        .json(&serde_json::json!({
            "exact": false,
            "filter": {"must": [{"key": "chunk_index", "match": { "value": 0 }}]}
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    Ok((info, count, docs_count))
}
