//! Static source-family matrix for public source contracts.
//!
//! Split into submodules to respect the monolith file-size policy: the scope
//! tables (one 17-field [`SourceScopeCapability`] per supported scope, see
//! `spec.rs`) live in `family_matrix/scopes_content.rs` (local/upload/git/
//! web/feed) and `family_matrix/scopes_tooling.rs` (youtube/reddit/sessions/
//! registry/cli_tool/mcp_tool/memory); the per-family [`SourceAdapterSpec`]
//! entries live in `family_matrix/matrix.rs`.

mod matrix;
mod scopes_content;
mod scopes_tooling;

use crate::spec::SourceAdapterSpec;

pub type SourceFamilyMatrix = &'static [SourceAdapterSpec];

pub fn source_family_matrix() -> SourceFamilyMatrix {
    matrix::source_family_matrix()
}
