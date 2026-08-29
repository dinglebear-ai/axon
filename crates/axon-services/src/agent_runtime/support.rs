use super::{AgentTurnOptions, MAX_TIMEOUT_MS, MAX_TOOL_CALLS, ModelAction, store::StoredTurn};
use axon_core::config::Config;

pub(super) fn build_model_prompt(turn: &StoredTurn) -> anyhow::Result<String> {
    Ok(format!(
        "You are Axon's bounded agent. Return exactly one JSON object: {{\"type\":\"final\",\"answer\":string}} or {{\"type\":\"tool\",\"tool_id\":string,\"contract_hash\":string,\"arguments\":object,\"destructive\":bool}}. Never invent tools outside the revision-bound loadout.\n\nUSER:\n{}\n\nTOOL RESULTS:\n{}",
        turn.prompt,
        serde_json::to_string(&turn.tool_results)?
    ))
}

pub(super) fn parse_action(text: &str) -> anyhow::Result<ModelAction> {
    let value = text
        .trim()
        .strip_prefix("```json")
        .and_then(|v| v.strip_suffix("```"))
        .unwrap_or(text.trim())
        .trim();
    serde_json::from_str(value).map_err(|_| anyhow::anyhow!("agent_model_contract_invalid"))
}

pub(super) fn validate_options(options: &AgentTurnOptions) -> anyhow::Result<()> {
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

pub(super) fn store_path(cfg: &Config) -> std::path::PathBuf {
    cfg.sqlite_path.with_file_name("agent-turns.sqlite3")
}

pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}
