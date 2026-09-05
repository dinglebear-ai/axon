//! Durable Axon-owned orchestration for Labby-executed agent tool calls.

mod client;
mod execution;
mod lease;
mod store;
mod support;

use axon_api::agent::{
    AgentApprovalToken, AgentCorrelation, AgentEvent, AgentToolProposal, AgentTurnOptions,
    AgentTurnResult, AgentTurnStatus,
};
use axon_core::config::Config;
use execution::{ensure_turn, run_loop};
use lease::await_with_renewal;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use support::{build_model_prompt, now_ms, parse_action, validate_options};

pub use client::{LabbyAgentClient, LabbyExecutionReceipt};
pub use store::AgentTurnStore;

const MAX_TOOL_CALLS: u32 = 32;
const MAX_TIMEOUT_MS: u64 = 15 * 60 * 1000;
const MAX_MODEL_OUTPUT_BYTES: usize = 1024 * 1024;

pub type CompletionFuture = Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>>;
pub type CompletionFn = Arc<dyn Fn(String) -> CompletionFuture + Send + Sync>;

async fn persist<T: Send + 'static>(
    store: &AgentTurnStore,
    operation: impl FnOnce(&AgentTurnStore) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    let store = store.clone();
    tokio::task::spawn_blocking(move || operation(&store))
        .await
        .map_err(|error| anyhow::anyhow!("agent persistence task failed: {error}"))?
}

#[derive(Debug, Clone)]
pub struct AgentTurnOwner {
    pub principal: String,
    pub profile_id: String,
}

pub fn configured_completion(cfg: Config) -> CompletionFn {
    Arc::new(move |prompt| {
        let cfg = cfg.clone();
        Box::pin(async move {
            let request = axon_llm::CompletionRequest::new(prompt)
                .backend_from_config_for(&cfg, axon_llm::LlmModelPurpose::Chat);
            Ok(axon_llm::complete_agent_text(request)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?
                .text)
        })
    })
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ModelAction {
    Final {
        answer: String,
    },
    Tool {
        tool_id: String,
        contract_hash: String,
        arguments: Value,
        #[serde(default)]
        destructive: bool,
    },
}

pub async fn run(
    cfg: &Config,
    loadout_id: &str,
    loadout_revision: u64,
    prompt: &str,
    options: AgentTurnOptions,
    owner: AgentTurnOwner,
    completion: CompletionFn,
) -> anyhow::Result<AgentTurnResult> {
    validate_options(&options)?;
    let store_cfg = cfg.clone();
    let store = tokio::task::spawn_blocking(move || AgentTurnStore::open(&store_cfg))
        .await
        .map_err(|error| anyhow::anyhow!("agent persistence task failed: {error}"))??;
    let turn_id = options
        .turn_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let requested_deadline = now_ms().saturating_add(options.timeout_ms as i64);
    let client = LabbyAgentClient::from_config(cfg)?;
    ensure_turn(
        &store,
        &client,
        &turn_id,
        loadout_id,
        loadout_revision,
        prompt,
        requested_deadline,
        &options.delegation_token,
        &owner,
        options.max_tool_calls,
        axon_core::llm::configured_chat_model_from_config(cfg)
            .as_deref()
            .unwrap_or("configured-default"),
    )
    .await?;
    let persisted_store = store.clone();
    let persisted_turn_id = turn_id.clone();
    let persisted_owner = owner.principal.clone();
    let persisted = tokio::task::spawn_blocking(move || {
        persisted_store.load_owned(&persisted_turn_id, &persisted_owner)
    })
    .await
    .map_err(|error| anyhow::anyhow!("agent persistence task failed: {error}"))??;
    if persisted.status.is_terminal() {
        let result_store = store.clone();
        let result_turn_id = turn_id.clone();
        return tokio::task::spawn_blocking(move || result_store.result(&result_turn_id))
            .await
            .map_err(|error| anyhow::anyhow!("agent persistence task failed: {error}"))?;
    }
    let lease_store = store.clone();
    let lease_turn_id = turn_id.clone();
    let lease_owner = owner.principal.clone();
    let lease_version = tokio::task::spawn_blocking(move || {
        lease_store.acquire_lease(&lease_turn_id, &lease_owner, persisted.version, now_ms())
    })
    .await
    .map_err(|error| anyhow::anyhow!("agent persistence task failed: {error}"))??;
    let approvals: HashMap<_, _> = options
        .approval_tokens
        .into_iter()
        .map(|v| (v.tool_call_id, v.token))
        .collect();
    run_loop(
        &store,
        &client,
        &turn_id,
        lease_version,
        &approvals,
        completion,
    )
    .await
}

pub async fn cancel(cfg: &Config, turn_id: &str, owner: &str) -> anyhow::Result<AgentTurnResult> {
    let store_cfg = cfg.clone();
    let turn_id_owned = turn_id.to_string();
    let owner_owned = owner.to_string();
    let (store, turn) = tokio::task::spawn_blocking(move || {
        let store = AgentTurnStore::open(&store_cfg)?;
        let turn = store.request_cancel(&turn_id_owned, &owner_owned)?;
        Ok::<_, anyhow::Error>((store, turn))
    })
    .await
    .map_err(|error| anyhow::anyhow!("agent persistence task failed: {error}"))??;
    if let Some(request_id) = turn.active_request_id.as_deref() {
        let receipt = LabbyAgentClient::from_config(cfg)?
            .cancel(request_id)
            .await?;
        if receipt.status != "cancelled" {
            anyhow::bail!("agent_cancel_unconfirmed");
        }
        let store_for_cancel = store.clone();
        let turn_id = turn_id.to_string();
        let owner = owner.to_string();
        tokio::task::spawn_blocking(move || store_for_cancel.confirm_cancel(&turn_id, &owner))
            .await
            .map_err(|error| anyhow::anyhow!("agent persistence task failed: {error}"))??;
    }
    let turn_id = turn_id.to_string();
    tokio::task::spawn_blocking(move || store.result(&turn_id))
        .await
        .map_err(|error| anyhow::anyhow!("agent persistence task failed: {error}"))?
}

pub async fn resume(
    cfg: &Config,
    turn_id: &str,
    owner: AgentTurnOwner,
    approvals: Vec<AgentApprovalToken>,
    completion: CompletionFn,
) -> anyhow::Result<AgentTurnResult> {
    let store_cfg = cfg.clone();
    let turn_id_owned = turn_id.to_string();
    let principal = owner.principal.clone();
    let turn = tokio::task::spawn_blocking(move || {
        AgentTurnStore::open(&store_cfg)?.load_owned(&turn_id_owned, &principal)
    })
    .await
    .map_err(|error| anyhow::anyhow!("agent persistence task failed: {error}"))??;
    turn.verify_resume(&owner.principal)?;
    let configured_model = axon_core::llm::configured_chat_model_from_config(cfg)
        .unwrap_or_else(|| "configured-default".into());
    if turn.model != configured_model {
        anyhow::bail!("agent_turn_model_binding_changed");
    }
    run(
        cfg,
        &turn.loadout_id,
        turn.loadout_revision,
        &turn.prompt,
        AgentTurnOptions {
            delegation_token: "persisted-context".into(),
            turn_id: Some(turn.id),
            approval_tokens: approvals,
            max_tool_calls: turn.max_tool_calls,
            timeout_ms: 1,
        },
        owner,
        completion,
    )
    .await
}

pub async fn status(cfg: &Config, turn_id: &str, owner: &str) -> anyhow::Result<AgentTurnResult> {
    let cfg = cfg.clone();
    let turn_id = turn_id.to_string();
    let owner = owner.to_string();
    tokio::task::spawn_blocking(move || {
        let store = AgentTurnStore::open(&cfg)?;
        store.load_owned(&turn_id, &owner)?;
        store.result(&turn_id)
    })
    .await
    .map_err(|error| anyhow::anyhow!("agent persistence task failed: {error}"))?
}
pub async fn events(
    cfg: &Config,
    turn_id: &str,
    owner: &str,
    after: u64,
) -> anyhow::Result<Vec<AgentEvent>> {
    let cfg = cfg.clone();
    let turn_id = turn_id.to_string();
    let owner = owner.to_string();
    tokio::task::spawn_blocking(move || {
        let store = AgentTurnStore::open(&cfg)?;
        store.load_owned(&turn_id, &owner)?;
        store.events(&turn_id, after)
    })
    .await
    .map_err(|error| anyhow::anyhow!("agent persistence task failed: {error}"))?
}

#[cfg(test)]
#[path = "agent_runtime_tests.rs"]
mod tests;
