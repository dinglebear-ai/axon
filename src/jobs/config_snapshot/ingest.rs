use serde::{Deserialize, Serialize};

use super::{ConfigSnapshot, serde_json_error};
use crate::{core::config::Config, jobs::ingest::IngestSource};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct IngestConfigSnapshotEnvelope {
    version: u8,
    source: Option<IngestSource>,
    config: ConfigSnapshot,
}

pub(crate) fn ingest_config_json(
    cfg: &Config,
    source: &IngestSource,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&IngestConfigSnapshotEnvelope {
        version: 2,
        source: Some(source.clone()),
        config: ConfigSnapshot::from_config(cfg).map_err(serde_json_error)?,
    })
}

pub(crate) fn decode_ingest_job_config(
    process_cfg: &Config,
    config_json: &str,
) -> Result<(IngestSource, Config), Box<dyn std::error::Error + Send + Sync>> {
    if let Ok(envelope) = serde_json::from_str::<IngestConfigSnapshotEnvelope>(config_json)
        && let Some(source) = envelope.source
    {
        let mut cfg = process_cfg.clone();
        let exact_options = envelope.version >= 2;
        envelope.config.apply_to(&mut cfg, exact_options)?;
        return Ok((source, cfg));
    }

    let source: IngestSource = serde_json::from_str(config_json)?;
    Ok((source, process_cfg.clone()))
}
