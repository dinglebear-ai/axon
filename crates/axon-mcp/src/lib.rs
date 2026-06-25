#![recursion_limit = "512"]

#[path = "auth.rs"]
pub mod auth;
#[path = "cors.rs"]
mod cors;
#[path = "schema.rs"]
pub mod schema;
#[path = "server.rs"]
pub mod server;

pub use auth::AuthPolicy;
pub use server::{AxonMcpServer, run_stdio_server};
