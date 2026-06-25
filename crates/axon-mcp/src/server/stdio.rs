use crate::auth::AuthPolicy;
use axon_core::config::Config;
use rmcp::{ServiceExt, transport::stdio};

use super::AxonMcpServer;

pub async fn run_stdio_server(cfg: Config) -> Result<(), Box<dyn std::error::Error>> {
    // Stdio always uses LoopbackDev: process isolation is the trust boundary.
    let server = AxonMcpServer::new(cfg).with_auth_policy(AuthPolicy::LoopbackDev);
    server
        .base_service_context()
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
