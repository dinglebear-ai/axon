#[path = "mcp/schema.rs"]
pub mod schema;
#[path = "mcp/server.rs"]
pub mod server;

pub use server::{AxonMcpServer, run_http_server, run_stdio_server};
