use crate::embed::validate_server_embed_input_with_config;
use crate::ingest::validate_ingest_source;
use axon_core::config::Config;
use axon_core::http::validate_url;
use axon_jobs::ingest::IngestSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "schema_version", rename_all = "snake_case")]
pub enum FreshnessRequestPayload {
    V1(FreshnessRequestV1),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum FreshnessRequestV1 {
    Scrape { url: String },
    Crawl { urls: Vec<String> },
    Embed { input: String },
    Ingest { source: IngestSource },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeReplayConfigV1 {
    pub collection: String,
    pub render_mode: String,
    pub max_pages: u32,
    pub max_depth: usize,
    pub include_subdomains: bool,
    pub custom_headers: Vec<String>,
    pub embed: bool,
    pub freshness_is_stripped: bool,
}

pub fn safe_replay_snapshot(
    cfg: &Config,
) -> Result<SafeReplayConfigV1, Box<dyn Error + Send + Sync>> {
    reject_secret_headers(&cfg.custom_headers)?;
    Ok(SafeReplayConfigV1 {
        collection: cfg.collection.clone(),
        render_mode: cfg.render_mode.to_string(),
        max_pages: cfg.max_pages,
        max_depth: cfg.max_depth,
        include_subdomains: cfg.include_subdomains,
        custom_headers: cfg.custom_headers.clone(),
        embed: cfg.embed,
        freshness_is_stripped: cfg.freshness.is_some(),
    })
}

pub fn validate_freshness_payload_for_dispatch(
    payload: &FreshnessRequestPayload,
    cfg: &Config,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match payload {
        FreshnessRequestPayload::V1(FreshnessRequestV1::Scrape { url }) => {
            validate_url(url).map_err(|err| err.to_string())?;
        }
        FreshnessRequestPayload::V1(FreshnessRequestV1::Crawl { urls }) => {
            if urls.is_empty() {
                return Err("freshness crawl payload requires at least one URL".into());
            }
            for url in urls {
                validate_url(url).map_err(|err| err.to_string())?;
            }
        }
        FreshnessRequestPayload::V1(FreshnessRequestV1::Embed { input }) => {
            validate_server_embed_input_with_config(cfg, input)?;
        }
        FreshnessRequestPayload::V1(FreshnessRequestV1::Ingest { source }) => {
            validate_ingest_source(source)?;
        }
    }
    Ok(())
}

pub fn freshness_identity_hash(
    command: &str,
    target: &str,
    every_seconds: i64,
    request_json: &Value,
    config_json: &Value,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(command.as_bytes());
    hasher.update(b"\0");
    hasher.update(target.as_bytes());
    hasher.update(b"\0");
    hasher.update(every_seconds.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(canonical_json(request_json).as_bytes());
    hasher.update(b"\0");
    hasher.update(canonical_json(config_json).as_bytes());
    to_hex(&hasher.finalize())
}

fn reject_secret_headers(headers: &[String]) -> Result<(), Box<dyn Error + Send + Sync>> {
    for header in headers {
        let Some((name, _value)) = header.split_once(':') else {
            continue;
        };
        if is_secret_header_name(name.trim()) {
            return Err("secret-bearing headers cannot be stored in freshness schedules".into());
        }
    }
    Ok(())
}

fn is_secret_header_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization" | "cookie" | "x-api-key" | "proxy-authorization" | "set-cookie"
    )
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<_, _> = map.iter().collect();
            let body = sorted
                .into_iter()
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("json key"),
                        canonical_json(value)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{body}}}")
        }
        Value::Array(values) => {
            let body = values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{body}]")
        }
        _ => serde_json::to_string(value).expect("json value"),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
