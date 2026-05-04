#[path = "mcp/auth.rs"]
pub(crate) mod auth;
#[path = "mcp/cors.rs"]
mod cors;
#[path = "mcp/schema.rs"]
pub mod schema;
#[path = "mcp/server.rs"]
pub mod server;

pub use server::{AxonMcpServer, run_http_server, run_stdio_server, run_unified_server};
