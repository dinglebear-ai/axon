use anyhow::Result;
use std::path::Path;

pub mod broken_symlinks;
pub mod claude_symlinks;
pub mod env_staged;
pub mod mcp_http;
pub mod no_mod_rs;
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
    version_sync::check(root)?;
    println!("All checks passed.");
    Ok(())
}
