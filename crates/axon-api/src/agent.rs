use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentTurnStatus {
    Pending,
    Proposing,
    AwaitingApproval,
    Executing,
    Continuing,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    Interrupted,
}

impl AgentTurnStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentTurnOptions {
    pub delegation_token: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub approval_tokens: Vec<AgentApprovalToken>,
    #[serde(default = "default_tool_budget")]
    pub max_tool_calls: u32,
    #[serde(default = "default_turn_timeout")]
    pub timeout_ms: u64,
}

fn default_tool_budget() -> u32 {
    8
}
fn default_turn_timeout() -> u64 {
    120_000
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentApprovalToken {
    pub tool_call_id: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolProposal {
    pub tool_call_id: String,
    pub tool_id: String,
    pub contract_hash: String,
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub destructive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentCorrelation {
    pub turn_id: String,
    pub execution_context_id: String,
    pub loadout_id: String,
    pub loadout_revision: u64,
    pub actor: String,
    pub service: String,
    pub tool_call_count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub request_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub receipt_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audit_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentTurnResult {
    pub turn_id: String,
    pub status: AgentTurnStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_approval: Option<AgentToolProposal>,
    pub correlation: AgentCorrelation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    State {
        sequence: u64,
        status: AgentTurnStatus,
    },
    ModelProposal {
        sequence: u64,
        proposal: AgentToolProposal,
    },
    AxonDecision {
        sequence: u64,
        decision: String,
    },
    LabbyExecution {
        sequence: u64,
        request_id: String,
        receipt_id: String,
        audit_id: String,
        status: String,
    },
    ToolResult {
        sequence: u64,
        tool_call_id: String,
        result: serde_json::Value,
    },
    Final {
        sequence: u64,
        answer: String,
    },
}
