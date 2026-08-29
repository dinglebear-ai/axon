use axon_api::agent::{AgentEvent, AgentTurnStatus};
use sha2::{Digest, Sha256};

pub(super) fn digest(v: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(v.as_bytes())))
}
pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}
pub(super) fn status_str(s: &AgentTurnStatus) -> &'static str {
    match s {
        AgentTurnStatus::Pending => "pending",
        AgentTurnStatus::Proposing => "proposing",
        AgentTurnStatus::AwaitingApproval => "awaiting_approval",
        AgentTurnStatus::Executing => "executing",
        AgentTurnStatus::Continuing => "continuing",
        AgentTurnStatus::Succeeded => "succeeded",
        AgentTurnStatus::Failed => "failed",
        AgentTurnStatus::Cancelled => "cancelled",
        AgentTurnStatus::TimedOut => "timed_out",
        AgentTurnStatus::Interrupted => "interrupted",
        AgentTurnStatus::CancelUnconfirmed => "cancel_unconfirmed",
    }
}
pub(super) fn parse_status(s: &str) -> AgentTurnStatus {
    match s {
        "pending" => AgentTurnStatus::Pending,
        "proposing" => AgentTurnStatus::Proposing,
        "awaiting_approval" => AgentTurnStatus::AwaitingApproval,
        "executing" => AgentTurnStatus::Executing,
        "continuing" => AgentTurnStatus::Continuing,
        "succeeded" => AgentTurnStatus::Succeeded,
        "cancelled" => AgentTurnStatus::Cancelled,
        "timed_out" => AgentTurnStatus::TimedOut,
        "interrupted" => AgentTurnStatus::Interrupted,
        "cancel_unconfirmed" => AgentTurnStatus::CancelUnconfirmed,
        _ => AgentTurnStatus::Failed,
    }
}
pub(super) fn with_sequence(e: AgentEvent, s: u64) -> AgentEvent {
    match e {
        AgentEvent::State { status, .. } => AgentEvent::State {
            sequence: s,
            status,
        },
        AgentEvent::ModelProposal { proposal, .. } => AgentEvent::ModelProposal {
            sequence: s,
            proposal,
        },
        AgentEvent::AxonDecision { decision, .. } => AgentEvent::AxonDecision {
            sequence: s,
            decision,
        },
        AgentEvent::LabbyExecution {
            request_id,
            receipt_id,
            audit_id,
            status,
            ..
        } => AgentEvent::LabbyExecution {
            sequence: s,
            request_id,
            receipt_id,
            audit_id,
            status,
        },
        AgentEvent::ToolResult {
            tool_call_id,
            result,
            ..
        } => AgentEvent::ToolResult {
            sequence: s,
            tool_call_id,
            result,
        },
        AgentEvent::Final { answer, .. } => AgentEvent::Final {
            sequence: s,
            answer,
        },
    }
}
