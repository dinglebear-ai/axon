#[path = "serve_supervisor.rs"]
mod serve_supervisor;

use crate::crates::core::config::Config;
use std::error::Error;
use std::sync::Arc;

pub async fn run_serve(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if serve_supervisor::is_internal_bridge_runtime() {
        return crate::crates::web::start_server(cfg.serve_port, Arc::new(cfg.clone())).await;
    }
    serve_supervisor::run_supervisor(cfg).await
}
