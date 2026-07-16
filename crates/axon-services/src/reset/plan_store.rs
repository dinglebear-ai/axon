use super::*;
use axon_core::redact::Redactor;

fn control_root(cfg: &Config) -> PathBuf {
    cfg.sqlite_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("reset-control")
}

fn validate_id(id: &str, prefix: &str) -> Result<(), Box<dyn Error>> {
    if !id.starts_with(prefix)
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(format!("invalid {prefix} identifier").into());
    }
    Ok(())
}

pub(super) fn plan_path(cfg: &Config, plan_id: &str) -> Result<PathBuf, Box<dyn Error>> {
    validate_id(plan_id, "reset_plan_")?;
    Ok(control_root(cfg)
        .join("plans")
        .join(format!("{plan_id}.json")))
}

pub(super) fn receipt_path(cfg: &Config, reset_id: &str) -> Result<PathBuf, Box<dyn Error>> {
    validate_id(reset_id, "reset_")?;
    Ok(control_root(cfg)
        .join("receipts")
        .join(format!("{reset_id}.json")))
}

pub(super) async fn save_plan(
    cfg: &Config,
    result: &ResetResult,
) -> Result<String, Box<dyn Error>> {
    let path = plan_path(cfg, &result.plan_id)?;
    let bytes = serde_json::to_vec_pretty(result)?;
    atomic_write(&path, &bytes).await?;
    Ok(path.display().to_string())
}

pub(super) async fn load_plan(cfg: &Config, plan_id: &str) -> Result<ResetResult, Box<dyn Error>> {
    let path = plan_path(cfg, plan_id)?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| format!("reset.plan_not_found: {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("reset.plan_invalid: {}: {error}", path.display()).into())
}

pub(super) async fn load_receipt(
    cfg: &Config,
    reset_id: &str,
) -> Result<Option<ResetReceipt>, Box<dyn Error>> {
    let path = receipt_path(cfg, reset_id)?;
    match tokio::fs::read(&path).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| format!("reset.receipt_invalid: {}: {error}", path.display()).into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) async fn save_receipt(
    cfg: &Config,
    receipt: &ResetReceipt,
) -> Result<String, Box<dyn Error>> {
    let path = receipt_path(cfg, &receipt.reset_id)?;
    let value = serde_json::to_value(receipt)?;
    let (value, _) = axon_core::redact::DefaultRedactor::new().redact_json(
        value,
        &axon_core::redact::RedactionContext::artifact_metadata(),
    );
    let bytes = serde_json::to_vec_pretty(&value)?;
    atomic_write(&path, &bytes).await?;
    Ok(path.display().to_string())
}

async fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let parent = path.parent().ok_or("reset control path has no parent")?;
    tokio::fs::create_dir_all(parent).await?;
    let tmp = path.with_extension("json.tmp");
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}
