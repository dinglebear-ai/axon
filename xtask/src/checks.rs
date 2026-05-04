use anyhow::Result;
use std::path::Path;

pub mod claude_symlinks;
pub mod env_staged;
pub mod mcp_http;
pub mod no_mod_rs;
pub mod unwraps;

pub fn check(root: &Path) -> Result<()> {
    no_mod_rs::check(root)?;
    mcp_http::check(root)?;
    env_staged::check(root)?;
    unwraps::check(root)?;
    claude_symlinks::check(root)?;
    println!("All checks passed.");
    Ok(())
}
