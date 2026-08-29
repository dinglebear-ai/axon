//! Durable Axon-owned orchestration for Labby-executed agent tool calls.

mod client;
mod store;

use axon_api::agent::{
    AgentApprovalToken, AgentCorrelation, AgentEvent, AgentToolProposal, AgentTurnOptions,
    AgentTurnResult, AgentTurnStatus,
};
use axon_core::config::Config;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub use client::{LabbyAgentClient, LabbyExecutionReceipt};
pub use store::AgentTurnStore;

const MAX_TOOL_CALLS: u32 = 32;
const MAX_TIMEOUT_MS: u64 = 15 * 60 * 1000;
const MAX_MODEL_OUTPUT_BYTES: usize = 1024 * 1024;

pub type CompletionFuture = Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>>;
pub type CompletionFn = Arc<dyn Fn(String) -> CompletionFuture + Send + Sync>;

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
    let store = AgentTurnStore::open(store_path(cfg))?;
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
    let persisted = store.load_owned(&turn_id, &owner.principal)?;
    if persisted.status.is_terminal() {
        return store.result(&turn_id);
    }
    let lease_version =
        store.acquire_lease(&turn_id, &owner.principal, persisted.version, now_ms())?;
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

async fn run_loop(
    store: &AgentTurnStore,
    client: &LabbyAgentClient,
    turn_id: &str,
    lease_version: u64,
    approvals: &HashMap<String, String>,
    completion: CompletionFn,
) -> anyhow::Result<AgentTurnResult> {
    loop {
        let mut turn = store.load(turn_id)?.expect("persisted turn");
        if turn.cancel_requested || turn.status == AgentTurnStatus::Cancelled {
            store.release_lease(turn_id, lease_version)?;
            return store.result(turn_id);
        }
        if now_ms() >= turn.deadline_at_ms {
            store.transition(turn_id, AgentTurnStatus::TimedOut, None)?;
            store.release_lease(turn_id, lease_version)?;
            return store.result(turn_id);
        }
        if turn.tool_call_count >= turn.max_tool_calls {
            store.transition(
                turn_id,
                AgentTurnStatus::Failed,
                Some("tool_budget_exceeded"),
            )?;
            store.release_lease(turn_id, lease_version)?;
            return store.result(turn_id);
        }
        if let Some(pending) = turn.pending_proposal.clone() {
            let approval = approvals.get(&pending.tool_call_id).map(String::as_str);
            if pending.destructive && approval.is_none() {
                store.release_lease(turn_id, lease_version)?;
                return store.result(turn_id);
            }
            execute_proposal(&store, &client, &mut turn, pending, approval).await?;
            continue;
        }
        store.transition(
            turn_id,
            if turn.tool_call_count == 0 {
                AgentTurnStatus::Proposing
            } else {
                AgentTurnStatus::Continuing
            },
            None,
        )?;
        let model_prompt = build_model_prompt(&turn)?;
        let output = tokio::time::timeout(
            Duration::from_millis((turn.deadline_at_ms - now_ms()).max(1) as u64),
            completion(model_prompt),
        )
        .await
        .map_err(|_| anyhow::anyhow!("agent_deadline_exceeded"))??;
        if store
            .load(turn_id)?
            .is_some_and(|value| value.cancel_requested)
        {
            store.release_lease(turn_id, lease_version)?;
            return store.result(turn_id);
        }
        if output.len() > MAX_MODEL_OUTPUT_BYTES {
            anyhow::bail!("agent_model_output_too_large");
        }
        match parse_action(&output)? {
            ModelAction::Final { answer } => {
                store.append_event(
                    turn_id,
                    AgentEvent::Final {
                        sequence: 0,
                        answer: answer.clone(),
                    },
                )?;
                store.transition(turn_id, AgentTurnStatus::Succeeded, Some(&answer))?;
                store.release_lease(turn_id, lease_version)?;
                return store.result(turn_id);
            }
            ModelAction::Tool {
                tool_id,
                contract_hash,
                arguments,
                destructive,
            } => {
                let proposal = AgentToolProposal {
                    tool_call_id: format!("{}:{}", turn_id, turn.tool_call_count + 1),
                    tool_id,
                    contract_hash,
                    arguments,
                    destructive,
                };
                store.set_proposal(turn_id, &proposal)?;
                if destructive && !approvals.contains_key(&proposal.tool_call_id) {
                    store.transition(turn_id, AgentTurnStatus::AwaitingApproval, None)?;
                    store.release_lease(turn_id, lease_version)?;
                    return store.result(turn_id);
                }
                continue;
            }
        }
    }
}

async fn ensure_turn(
    store: &AgentTurnStore,
    client: &LabbyAgentClient,
    turn_id: &str,
    loadout_id: &str,
    loadout_revision: u64,
    prompt: &str,
    deadline: i64,
    delegation_token: &str,
    owner: &AgentTurnOwner,
    max_tool_calls: u32,
    model: &str,
) -> anyhow::Result<()> {
    if let Some(existing) = store.load(turn_id)? {
        return existing.verify_create_replay(
            &owner.principal,
            &owner.profile_id,
            loadout_id,
            loadout_revision,
            prompt,
        );
    }
    let context = client
        .create_context(delegation_token, loadout_id, loadout_revision, deadline)
        .await?;
    store.create(
        turn_id,
        loadout_id,
        loadout_revision,
        prompt,
        deadline,
        &owner.principal,
        &owner.profile_id,
        max_tool_calls,
        model,
        &context,
    )?;
    Ok(())
}

async fn execute_proposal(
    store: &AgentTurnStore,
    client: &LabbyAgentClient,
    turn: &mut store::StoredTurn,
    proposal: AgentToolProposal,
    approval: Option<&str>,
) -> anyhow::Result<()> {
    store.transition(&turn.id, AgentTurnStatus::Executing, None)?;
    let key = format!("axon-agent:{}:{}", turn.id, proposal.tool_call_id);
    let mut receipt = match store.execution_request_id(&turn.id, &proposal.tool_call_id)? {
        Some(request_id) => client.status(&request_id).await?,
        None => {
            store.reserve_execution(&turn.id, &proposal.tool_call_id, &key)?;
            client
                .execute(
                    &turn.execution_context_id,
                    &key,
                    &proposal,
                    approval,
                    turn.deadline_at_ms,
                )
                .await?
        }
    };
    // Persist Labby's request identity before any cancellation check or status await.
    // A crash after dispatch therefore resumes through status, while a crash before
    // this write safely replays the distinct idempotency key through Labby.
    store.record_receipt(&turn.id, &proposal, &receipt)?;
    if store
        .load(&turn.id)?
        .is_some_and(|value| value.cancel_requested)
    {
        let cancelled = client.cancel(&receipt.request_id).await;
        return match cancelled {
            Ok(value) if value.status == "cancelled" => {
                store.confirm_cancel(&turn.id, &turn.owner)?;
                Ok(())
            }
            _ => Ok(()),
        };
    }
    while receipt.status == "running" && now_ms() < turn.deadline_at_ms {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if store
            .load(&turn.id)?
            .is_some_and(|value| value.status == AgentTurnStatus::Cancelled)
        {
            receipt = client.cancel(&receipt.request_id).await?;
            break;
        }
        receipt = client.status(&receipt.request_id).await?;
    }
    store.record_receipt(&turn.id, &proposal, &receipt)?;
    match receipt.status.as_str() {
        "succeeded" => store.complete_tool(
            &turn.id,
            &proposal.tool_call_id,
            receipt.result.unwrap_or(Value::Null),
        )?,
        "running" => store.transition(
            &turn.id,
            AgentTurnStatus::TimedOut,
            Some("labby_status_deadline"),
        )?,
        "cancelled" => store.transition(
            &turn.id,
            AgentTurnStatus::Cancelled,
            Some("labby_cancelled"),
        )?,
        "timed_out" => {
            store.transition(&turn.id, AgentTurnStatus::TimedOut, Some("labby_timed_out"))?
        }
        _ => store.transition(
            &turn.id,
            AgentTurnStatus::Failed,
            receipt.error_kind.as_deref(),
        )?,
    }
    Ok(())
}

fn build_model_prompt(turn: &store::StoredTurn) -> anyhow::Result<String> {
    Ok(format!(
        "You are Axon's bounded agent. Return exactly one JSON object: {{\"type\":\"final\",\"answer\":string}} or {{\"type\":\"tool\",\"tool_id\":string,\"contract_hash\":string,\"arguments\":object,\"destructive\":bool}}. Never invent tools outside the revision-bound loadout.\n\nUSER:\n{}\n\nTOOL RESULTS:\n{}",
        turn.prompt,
        serde_json::to_string(&turn.tool_results)?
    ))
}

fn parse_action(text: &str) -> anyhow::Result<ModelAction> {
    let value = text
        .trim()
        .strip_prefix("```json")
        .and_then(|v| v.strip_suffix("```"))
        .unwrap_or(text.trim())
        .trim();
    serde_json::from_str(value).map_err(|_| anyhow::anyhow!("agent_model_contract_invalid"))
}

fn validate_options(options: &AgentTurnOptions) -> anyhow::Result<()> {
    if options.delegation_token.is_empty()
        || options.delegation_token.len() > 512
        || options.max_tool_calls == 0
        || options.max_tool_calls > MAX_TOOL_CALLS
        || options.timeout_ms == 0
        || options.timeout_ms > MAX_TIMEOUT_MS
    {
        anyhow::bail!("agent_bounds_invalid");
    }
    Ok(())
}

fn store_path(cfg: &Config) -> std::path::PathBuf {
    cfg.sqlite_path.with_file_name("agent-turns.sqlite3")
}
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

pub async fn cancel(cfg: &Config, turn_id: &str, owner: &str) -> anyhow::Result<AgentTurnResult> {
    let store = AgentTurnStore::open(store_path(cfg))?;
    let turn = store.request_cancel(turn_id, owner)?;
    if let Some(request_id) = turn.active_request_id.as_deref() {
        let receipt = LabbyAgentClient::from_config(cfg)?
            .cancel(request_id)
            .await?;
        if receipt.status != "cancelled" {
            anyhow::bail!("agent_cancel_unconfirmed");
        }
        store.confirm_cancel(turn_id, owner)?;
    }
    store.result(turn_id)
}

pub async fn resume(
    cfg: &Config,
    turn_id: &str,
    owner: AgentTurnOwner,
    approvals: Vec<AgentApprovalToken>,
    completion: CompletionFn,
) -> anyhow::Result<AgentTurnResult> {
    let store = AgentTurnStore::open(store_path(cfg))?;
    let turn = store.load_owned(turn_id, &owner.principal)?;
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

pub fn status(cfg: &Config, turn_id: &str, owner: &str) -> anyhow::Result<AgentTurnResult> {
    let store = AgentTurnStore::open(store_path(cfg))?;
    store.load_owned(turn_id, owner)?;
    store.result(turn_id)
}
pub fn events(
    cfg: &Config,
    turn_id: &str,
    owner: &str,
    after: u64,
) -> anyhow::Result<Vec<AgentEvent>> {
    let store = AgentTurnStore::open(store_path(cfg))?;
    store.load_owned(turn_id, owner)?;
    store.events(turn_id, after)
}

#[cfg(test)]
#[path = "agent_runtime_tests.rs"]
mod tests;
