use crate::context::ServiceContext;
use crate::embed::validate_server_embed_input_with_config;
use crate::ingest::{classify_target, validate_ingest_source};
use axon_api::ingest::target_label;
use axon_core::config::{CommandKind, Config, FreshnessCommand};
use axon_core::http::validate_url;
use axon_core::redact::is_secret_like;
use axon_jobs::config_snapshot::{apply_config_snapshot, config_snapshot_json};
use axon_jobs::freshness::{
    FreshnessDef, FreshnessDefCreate, FreshnessRun, create_freshness_def_with_pool,
    list_freshness_defs_with_pool, list_freshness_runs_with_pool,
};
use axon_jobs::ingest::IngestSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::error::Error;
use uuid::Uuid;

mod scheduler;
#[cfg(test)]
pub(crate) use scheduler::dispatch_freshness;
#[cfg(test)]
pub(crate) use scheduler::lease_limit_for_available_capacity;
pub use scheduler::{run_now, spawn_freshness_scheduler};

pub(crate) type FreshnessError = Box<dyn Error + Send + Sync>;

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
    #[serde(default)]
    pub config_snapshot_json: Option<String>,
    pub collection: String,
    pub render_mode: String,
    pub max_pages: u32,
    pub max_depth: usize,
    pub include_subdomains: bool,
    pub custom_headers: Vec<String>,
    pub embed: bool,
    pub freshness_is_stripped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreshnessDispatchOutcome {
    pub status: String,
    pub dispatched_job_id: Option<Uuid>,
    pub result_json: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreshnessCreated {
    pub id: Uuid,
    pub name: String,
    pub command: String,
    pub target: String,
    pub identity_hash: String,
    pub every_seconds: i64,
    pub next_run_at: chrono::DateTime<chrono::Utc>,
    pub request_json: Value,
    pub config_json: Value,
}

impl From<FreshnessDef> for FreshnessCreated {
    fn from(def: FreshnessDef) -> Self {
        Self {
            id: def.id,
            name: def.name,
            command: def.command,
            target: def.target,
            identity_hash: def.identity_hash,
            every_seconds: def.every_seconds,
            next_run_at: def.next_run_at,
            request_json: def.request_json,
            config_json: def.config_json,
        }
    }
}

pub fn safe_replay_snapshot(
    cfg: &Config,
) -> Result<SafeReplayConfigV1, Box<dyn Error + Send + Sync>> {
    reject_secret_headers(&cfg.custom_headers)?;
    Ok(SafeReplayConfigV1 {
        config_snapshot_json: Some(config_snapshot_json(cfg)?),
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

pub async fn create_from_config(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<FreshnessCreated, FreshnessError> {
    let Some(intent) = cfg.freshness.as_ref() else {
        return Err("freshness intent is required".into());
    };
    let (command, target, request_payload) = request_payload_from_config(cfg, intent.command)?;
    validate_freshness_payload_for_dispatch(&request_payload, cfg)?;
    let replay = safe_replay_snapshot(cfg)?;
    let request_json = serde_json::to_value(&request_payload)?;
    let config_json = serde_json::to_value(&replay)?;
    let identity_hash = freshness_identity_hash(
        command,
        &target,
        intent.every_seconds,
        &request_json,
        &config_json,
    );
    let pool = service_context
        .jobs
        .sqlite_pool()
        .ok_or("freshness schedules require the SQLite job runtime")?;
    let def = create_freshness_def_with_pool(
        &pool,
        &FreshnessDefCreate {
            name: format!("{command}:{target}"),
            command: command.to_string(),
            target,
            identity_hash,
            request_json,
            config_json,
            every_seconds: intent.every_seconds,
            enabled: true,
            next_run_at: None,
        },
    )
    .await
    .map_err(to_freshness_error)?;
    Ok(def.into())
}

pub async fn list(
    service_context: &ServiceContext,
    limit: i64,
) -> Result<Vec<FreshnessDef>, FreshnessError> {
    let pool = service_context
        .jobs
        .sqlite_pool()
        .ok_or("freshness schedules require the SQLite job runtime")?;
    list_freshness_defs_with_pool(&pool, limit)
        .await
        .map_err(to_freshness_error)
}

pub async fn history(
    service_context: &ServiceContext,
    id: Uuid,
    limit: i64,
) -> Result<Vec<FreshnessRun>, FreshnessError> {
    let pool = service_context
        .jobs
        .sqlite_pool()
        .ok_or("freshness schedules require the SQLite job runtime")?;
    list_freshness_runs_with_pool(&pool, id, limit)
        .await
        .map_err(to_freshness_error)
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

fn request_payload_from_config(
    cfg: &Config,
    command: FreshnessCommand,
) -> Result<(&'static str, String, FreshnessRequestPayload), FreshnessError> {
    match command {
        FreshnessCommand::Scrape => {
            if cfg.command != CommandKind::Scrape {
                return Err("freshness command mismatch: expected scrape".into());
            }
            let url = cfg
                .positional
                .first()
                .cloned()
                .ok_or("scrape freshness requires a URL")?;
            Ok((
                "scrape",
                url.clone(),
                FreshnessRequestPayload::V1(FreshnessRequestV1::Scrape { url }),
            ))
        }
        FreshnessCommand::Crawl => {
            if cfg.command != CommandKind::Crawl {
                return Err("freshness command mismatch: expected crawl".into());
            }
            if cfg.positional.is_empty() {
                return Err("crawl freshness requires at least one URL".into());
            }
            let urls = cfg.positional.clone();
            Ok((
                "crawl",
                urls.join(","),
                FreshnessRequestPayload::V1(FreshnessRequestV1::Crawl { urls }),
            ))
        }
        FreshnessCommand::Embed => {
            if cfg.command != CommandKind::Embed {
                return Err("freshness command mismatch: expected embed".into());
            }
            let input = cfg.positional.first().cloned().unwrap_or_else(|| {
                cfg.output_dir
                    .join("markdown")
                    .to_string_lossy()
                    .to_string()
            });
            Ok((
                "embed",
                input.clone(),
                FreshnessRequestPayload::V1(FreshnessRequestV1::Embed { input }),
            ))
        }
        FreshnessCommand::Ingest => {
            if cfg.command != CommandKind::Ingest {
                return Err("freshness command mismatch: expected ingest".into());
            }
            let target = cfg
                .positional
                .first()
                .cloned()
                .ok_or("ingest freshness requires a target")?;
            let source = classify_target(&target, cfg.github_include_source)
                .map_err(|err| -> FreshnessError { err.to_string().into() })?;
            let display_target = target_label(&source);
            Ok((
                "ingest",
                display_target,
                FreshnessRequestPayload::V1(FreshnessRequestV1::Ingest { source }),
            ))
        }
    }
}

pub(crate) fn replay_config(
    base: &Config,
    replay: &SafeReplayConfigV1,
) -> Result<Config, FreshnessError> {
    if let Some(config_snapshot_json) = replay.config_snapshot_json.as_deref() {
        let mut cfg = apply_config_snapshot(base, config_snapshot_json)?;
        cfg.freshness = None;
        cfg.wait = false;
        return Ok(cfg);
    }

    let mut cfg = base.clone();
    cfg.collection = replay.collection.clone();
    cfg.render_mode = serde_json::from_value(Value::String(replay.render_mode.clone()))?;
    cfg.max_pages = replay.max_pages;
    cfg.max_depth = replay.max_depth;
    cfg.include_subdomains = replay.include_subdomains;
    cfg.custom_headers = replay.custom_headers.clone();
    cfg.embed = replay.embed;
    cfg.freshness = None;
    cfg.wait = false;
    Ok(cfg)
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
    let lower = name.to_ascii_lowercase();
    lower == "cookie" || lower == "set-cookie" || is_secret_like(&lower)
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

pub(crate) fn freshness_lease_ttl_ms(cfg: &Config) -> i64 {
    cfg.freshness_lease_secs.max(1) as i64 * 1_000
}

pub(crate) fn to_freshness_error<E: std::fmt::Display>(error: E) -> FreshnessError {
    error.to_string().into()
}
