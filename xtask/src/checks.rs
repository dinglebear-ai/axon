use anyhow::Result;
use std::path::Path;

pub mod android_api_contract;
pub mod broken_symlinks;
pub mod claude_symlinks;
pub mod env_staged;
pub mod mcp_http;
pub mod no_mod_rs;
pub mod openapi_drift;
pub mod release_versions;
pub mod secrets;
pub mod unwraps;
pub mod version_sync;

pub fn check(root: &Path) -> Result<()> {
    no_mod_rs::check(root)?;
    mcp_http::check(root)?;
    env_staged::check(root)?;
    unwraps::check(root)?;
    claude_symlinks::check(root)?;
    broken_symlinks::check(root)?;
    secrets::check(root)?;
    release_versions::check_local(root)?;
    println!("All checks passed.");
    Ok(())
}
