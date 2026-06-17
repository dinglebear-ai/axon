use anyhow::Result;
use std::path::Path;

/// Compatibility command for the CLI component's parity set only.
///
/// The full multi-component release gate lives in `check-release-versions` and
/// is backed by `release/components.toml`.
pub fn check(root: &Path) -> Result<()> {
    Ok(super::release_versions::check_cli_parity_only(root)?)
}
