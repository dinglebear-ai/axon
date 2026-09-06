use std::path::Path;

pub(super) fn lease_name(claimed: &Path) -> anyhow::Result<String> {
    let stem = claimed
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow::anyhow!("claim has no UTF-8 UUID stem"))?;
    let id = uuid::Uuid::parse_str(stem)?;
    Ok(format!("lease-{:02x}.lock", id.as_bytes()[0]))
}

#[cfg(unix)]
pub(super) fn component_name(path: &Path) -> anyhow::Result<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt as _;
    let name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("journal path has no filename"))?;
    if path.components().next_back() != Some(std::path::Component::Normal(name)) {
        anyhow::bail!("journal filename is not one normal component");
    }
    Ok(name.as_bytes().to_vec())
}
