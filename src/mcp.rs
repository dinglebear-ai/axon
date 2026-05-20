#[path = "mcp/auth.rs"]
pub(crate) mod auth;
#[path = "mcp/cors.rs"]
mod cors;
#[path = "mcp/schema.rs"]
pub mod schema;
#[path = "mcp/server.rs"]
pub mod server;
#[path = "mcp/thin_client.rs"]
pub mod thin_client;

pub use auth::AuthPolicy;
pub use server::{AxonMcpServer, run_stdio_server, run_unified_server};

#[cfg(test)]
#[path = "mcp/thin_client_tests.rs"]
mod thin_client_tests;
