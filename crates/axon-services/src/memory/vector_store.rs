use anyhow::Result;
use serde_json::json;

use axon_core::config::Config;
use axon_core::http::internal_service_http_client;
use axon_vector::ops::qdrant::qdrant_base;

use super::MemoryItem;

pub(super) async fn update_qdrant_memory_status(
    cfg: &Config,
    id: &str,
    status: &str,
    now: i64,
) -> Result<()> {
    let client = internal_service_http_client()?;
    let url = format!(
        "{}/collections/{}/points/payload?wait=true",
        qdrant_base(cfg).trim_end_matches('/'),
        cfg.collection
    );
    client
        .post(url)
        .json(&json!({
            "payload": {
                "status": status,
                "updated_at": now,
                "last_seen_at": now
            },
            "points": [id]
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub(super) async fn retrieve_body_by_id(cfg: &Config, id: &str) -> Result<Option<String>> {
    let filter = json!({
        "must": [
            {"key": "memory", "match": {"value": true}},
            {"has_id": [id]}
        ]
    });
    let client = internal_service_http_client()?;
    let url = format!(
        "{}/collections/{}/points/scroll",
        qdrant_base(cfg).trim_end_matches('/'),
        cfg.collection
    );
    let response = client
        .post(url)
        .json(&json!({
            "limit": 1,
            "with_payload": true,
            "with_vector": false,
            "filter": filter
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    Ok(response
        .pointer("/result/points/0/payload/body")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string))
}

pub(super) async fn hydrate_memory_bodies(cfg: &Config, items: &mut [MemoryItem]) -> Result<()> {
    for item in items {
        item.body = retrieve_body_by_id(cfg, &item.id).await?;
    }
    Ok(())
}

pub(super) fn memory_filter(
    project: Option<&str>,
    repo: Option<&str>,
    file: Option<&str>,
) -> serde_json::Value {
    let mut must = vec![
        json!({"key": "memory", "match": {"value": true}}),
        json!({"key": "status", "match": {"value": "active"}}),
    ];
    if let Some(project) = project.filter(|v| !v.trim().is_empty()) {
        must.push(json!({"key": "project", "match": {"value": project}}));
    }
    if let Some(repo) = repo.filter(|v| !v.trim().is_empty()) {
        must.push(json!({"key": "repo", "match": {"value": repo}}));
    }
    if let Some(file) = file.filter(|v| !v.trim().is_empty()) {
        must.push(json!({"key": "file", "match": {"value": file}}));
    }
    json!({ "must": must })
}
