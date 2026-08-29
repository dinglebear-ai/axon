use axon_api::loadout::{LoadoutBinding, LoadoutResolution, LoadoutResolutionStatus};
use axon_core::config::Config;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::Duration;

const MAX_CONTEXT_CHARS: usize = 16_384;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Preview {
    loadout_id: String,
    draft_revision: u64,
    catalog_generation: String,
    runtime_identity: String,
    effective: Vec<CapabilityRef>,
    missing: Vec<CapabilityRef>,
    conflicts: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapabilityRef {
    provider: String,
    family: String,
    member_id: String,
    expected_revision: String,
}

pub struct ResolvedLoadoutContext {
    pub metadata: LoadoutResolution,
    pub prompt_context: String,
}

pub async fn resolve(
    cfg: &Config,
    binding: &LoadoutBinding,
) -> anyhow::Result<ResolvedLoadoutContext> {
    validate_binding(binding)?;
    let configured_id = cfg.labby_integration_id.as_deref().ok_or_else(|| {
        anyhow::anyhow!("loadout_backend_missing: Labby integration is not configured")
    })?;
    if binding.integration_id != configured_id {
        anyhow::bail!("loadout_backend_missing: unknown Labby integration id");
    }
    let base = cfg.labby_url.as_deref().ok_or_else(|| {
        anyhow::anyhow!("loadout_backend_missing: Labby endpoint is not configured")
    })?;
    let token = cfg.labby_service_token.as_deref().ok_or_else(|| {
        anyhow::anyhow!("loadout_auth_denied: Labby service credential is not configured")
    })?;
    let runtime_identity = cfg.labby_runtime_identity.as_deref().ok_or_else(|| {
        anyhow::anyhow!("loadout_backend_missing: Labby runtime identity is not configured")
    })?;
    let mut url = reqwest::Url::parse(base)
        .map_err(|_| anyhow::anyhow!("loadout_backend_invalid: invalid Labby URL"))?;
    if url.scheme() != "https"
        && url.host_str() != Some("127.0.0.1")
        && url.host_str() != Some("localhost")
    {
        anyhow::bail!("loadout_backend_invalid: Labby URL must use HTTPS or loopback HTTP");
    }
    url.set_path(&format!(
        "/v1/palette/execution-loadouts/{}/preview",
        percent_encoding::utf8_percent_encode(
            &binding.loadout_id,
            percent_encoding::NON_ALPHANUMERIC
        )
    ));
    url.set_query(None);
    let correlation_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(cfg.labby_resolution_timeout_ms))
        .build()?;
    let response = client
        .post(url)
        .bearer_auth(token)
        .header("x-request-id", &correlation_id)
        .json(&serde_json::json!({ "runtimeIdentity": runtime_identity }))
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("loadout_unavailable: Labby resolution failed"))?;
    if !response.status().is_success() {
        anyhow::bail!(
            "loadout_resolution_failed: Labby returned {}",
            response.status().as_u16()
        );
    }
    if response
        .content_length()
        .is_some_and(|len| len > cfg.labby_resolution_max_bytes as u64)
    {
        anyhow::bail!("loadout_payload_too_large: Labby response exceeded configured limit");
    }
    let bytes = response.bytes().await?;
    if bytes.len() > cfg.labby_resolution_max_bytes {
        anyhow::bail!("loadout_payload_too_large: Labby response exceeded configured limit");
    }
    let preview: Preview = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("loadout_contract_invalid: invalid Labby preview"))?;
    if preview.loadout_id != binding.loadout_id || preview.runtime_identity != runtime_identity {
        anyhow::bail!("loadout_contract_invalid: Labby preview identity mismatch");
    }
    if preview.draft_revision != binding.expected_revision {
        anyhow::bail!(
            "loadout_revision_conflict: expected revision {}, current revision {}",
            binding.expected_revision,
            preview.draft_revision
        );
    }
    if !preview.conflicts.is_empty() {
        anyhow::bail!("loadout_unresolved: Labby reported capability conflicts");
    }
    let prompt_context = build_context(&preview.effective)?;
    let execution_context_id =
        context_id(&binding.integration_id, &preview, binding.expected_revision);
    let unavailable = preview.missing.len();
    Ok(ResolvedLoadoutContext {
        metadata: LoadoutResolution {
            integration_id: binding.integration_id.clone(),
            loadout_id: binding.loadout_id.clone(),
            requested_revision: binding.expected_revision,
            effective_revision: preview.draft_revision,
            catalog_generation: preview.catalog_generation,
            execution_context_id,
            correlation_id,
            status: if unavailable == 0 {
                LoadoutResolutionStatus::Effective
            } else {
                LoadoutResolutionStatus::Narrowed
            },
            effective_capability_count: preview.effective.len(),
            unavailable_capability_count: unavailable,
        },
        prompt_context,
    })
}

fn validate_binding(binding: &LoadoutBinding) -> anyhow::Result<()> {
    for (name, value) in [
        ("integration_id", &binding.integration_id),
        ("loadout_id", &binding.loadout_id),
    ] {
        if value.is_empty() || value.len() > 256 {
            anyhow::bail!("loadout_binding_invalid: {name}");
        }
    }
    if binding
        .conversation_binding
        .as_ref()
        .is_some_and(|value| value.len() > 256)
    {
        anyhow::bail!("loadout_binding_invalid: conversation_binding");
    }
    Ok(())
}

fn build_context(capabilities: &[CapabilityRef]) -> anyhow::Result<String> {
    let mut rows = capabilities
        .iter()
        .map(|item| {
            format!(
                "{}:{}:{}@{}",
                item.provider, item.family, item.member_id, item.expected_revision
            )
        })
        .collect::<Vec<_>>();
    rows.sort();
    let context = format!(
        "<labby_execution_context trust=\"untrusted_metadata_only\">\n{}\n</labby_execution_context>",
        rows.join("\n")
    );
    if context.len() > MAX_CONTEXT_CHARS {
        anyhow::bail!("loadout_context_too_large: resolved context exceeded limit");
    }
    Ok(context)
}

fn context_id(integration_id: &str, preview: &Preview, revision: u64) -> String {
    let mut hash = Sha256::new();
    hash.update(integration_id);
    hash.update(&preview.loadout_id);
    hash.update(revision.to_le_bytes());
    hash.update(&preview.catalog_generation);
    format!("ctx_{:x}", hash.finalize())
}

#[cfg(test)]
#[path = "loadout_context_tests.rs"]
mod tests;
