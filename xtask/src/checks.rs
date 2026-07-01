use anyhow::Result;
use std::path::Path;

pub mod android_api_contract;
pub mod api_parity;
pub mod broken_symlinks;
pub mod claude_symlinks;
pub mod env_staged;
pub mod layering;
pub mod mcp_http;
pub mod no_mod_rs;
pub mod openapi_drift;
pub mod release_versions;
pub mod repo_structure;
pub mod secrets;
pub mod sqlite_migrations;
pub mod unwraps;
pub mod version_sync;

#[cfg(test)]
mod repo_structure_tests;

pub fn check(root: &Path) -> Result<()> {
    no_mod_rs::check(root)?;
    layering::check(root)?;
    openapi_drift::check(root)?;
    api_parity::check(root)?;
    mcp_http::check(root)?;
    env_staged::check(root)?;
    unwraps::check(root)?;
    claude_symlinks::check(root)?;
    repo_structure::check(root)?;
    broken_symlinks::check(root)?;
    sqlite_migrations::check(root)?;
    secrets::check(root)?;
    release_versions::check_local(root)?;
    println!("All checks passed.");
    Ok(())
}
