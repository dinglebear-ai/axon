//! Compatibility shim: the MCP wire-contract DTOs now live in
//! `axon_api::mcp_schema`. Re-exported so existing `crate::schema::*`
//! call sites resolve unchanged.
pub use axon_api::mcp_schema::*;
