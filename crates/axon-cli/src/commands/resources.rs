//! Canonical operational resource commands from the pipeline-unification
//! command contract.

use axon_core::config::Config;
use axon_services::context::ServiceContext;
use std::error::Error;
use std::sync::Arc;

#[path = "resources/artifacts_uploads.rs"]
mod artifacts_uploads;
#[path = "resources/discovery.rs"]
mod discovery;
#[path = "resources/graph.rs"]
mod graph;

pub async fn run_artifacts(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    artifacts_uploads::run_artifacts(cfg, context).await
}

pub async fn run_uploads(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    artifacts_uploads::run_uploads(cfg, context).await
}

pub async fn run_collections(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    discovery::run_collections(cfg, context).await
}

pub async fn run_graph(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    graph::run_graph(cfg, context).await
}

pub async fn run_providers(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    discovery::run_providers(cfg, context).await
}

pub async fn run_capabilities(cfg: &Config) -> Result<(), Box<dyn Error>> {
    discovery::run_capabilities(cfg)
}

pub async fn run_chat(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    use axon_services::service_traits::{AskService, AskServiceImpl};
    let message = cfg.positional.join(" ");
    let result = AskServiceImpl::new(Arc::new(context.clone()))
        .chat(axon_services::service_traits::ask_service::ChatRequest {
            session_id: None,
            message,
        })
        .await?;
    if cfg.json_output {
        print_value(result)
    } else {
        println!("{}", result.reply);
        Ok(())
    }
}

pub(super) fn flag_value(cfg: &Config, name: &str) -> Option<String> {
    cfg.positional
        .windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

pub(super) fn flag_values(cfg: &Config, name: &str) -> Vec<String> {
    cfg.positional
        .windows(2)
        .filter(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .collect()
}

pub(super) fn parse_u32_flag(cfg: &Config, name: &str) -> Result<Option<u32>, Box<dyn Error>> {
    flag_value(cfg, name)
        .map(|value| value.parse::<u32>().map_err(Into::into))
        .transpose()
}

pub(super) fn positional<'a>(
    cfg: &'a Config,
    index: usize,
    label: &str,
) -> Result<&'a str, Box<dyn Error>> {
    cfg.positional
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{label} is required").into())
}

pub(super) fn print_value(value: impl serde::Serialize) -> Result<(), Box<dyn Error>> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
