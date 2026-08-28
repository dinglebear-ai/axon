//! Machine-readable Codex app-server capability policy.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Checked-in method inventory generated from the pinned development Codex CLI.
pub const METHOD_INVENTORY_JSON: &str =
    include_str!("../../../docs/reference/codex-app-server-methods.json");

/// Wire-message direction in the app-server protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    ClientRequest,
    ServerRequest,
    ServerNotification,
}

/// Security classification applied before a method is dispatched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    SafeRead,
    SensitiveRead,
    Mutation,
    Execution,
    ApprovalRequired,
    Unsupported,
    Deferred,
}

/// Product family used by Palette, doctor, and policy configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityFamily {
    Runtime,
    Account,
    Config,
    Models,
    Mcp,
    Skills,
    Hooks,
    Plugins,
    Marketplace,
    Apps,
    ExternalAgentConfig,
    Filesystem,
    Command,
    Process,
    Project,
    RemoteControl,
    Threads,
    Realtime,
    Review,
    WindowsSandbox,
    Approval,
    System,
    Other,
}

/// Static support decision for one app-server method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodCapability {
    pub method: String,
    pub kind: MessageKind,
    pub family: CapabilityFamily,
    pub risk: RiskClass,
    pub compiled: bool,
    pub policy_enabled: bool,
    pub server_advertised: bool,
    pub ready: bool,
    pub schema_version: String,
    pub codex_cli_version: String,
}

/// A deterministic difference between generated schema methods and Axon's snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaDrift {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl SchemaDrift {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

pub fn diff_methods(expected: &[&str], generated: &[String]) -> SchemaDrift {
    let expected: BTreeSet<_> = expected.iter().map(|value| (*value).to_owned()).collect();
    let generated: BTreeSet<_> = generated.iter().cloned().collect();
    SchemaDrift {
        added: generated.difference(&expected).cloned().collect(),
        removed: expected.difference(&generated).cloned().collect(),
    }
}

/// Classify every known family, failing closed for an unknown method family.
pub fn classify(method: &str, kind: MessageKind) -> (CapabilityFamily, RiskClass) {
    let family = family_for(method);
    let risk = match kind {
        MessageKind::ServerRequest if is_approval(method) => RiskClass::ApprovalRequired,
        MessageKind::ServerRequest => RiskClass::Unsupported,
        MessageKind::ServerNotification => RiskClass::SafeRead,
        MessageKind::ClientRequest => request_risk(method, family),
    };
    (family, risk)
}

fn request_risk(method: &str, family: CapabilityFamily) -> RiskClass {
    if matches!(
        method,
        "initialize" | "model/list" | "mcpServerStatus/list" | "hooks/list"
    ) || method.ends_with("/list")
        || method.ends_with("/readiness")
        || method.ends_with("/status/read")
    {
        return RiskClass::SafeRead;
    }
    if method.ends_with("/read") || method.contains("usage") || method.contains("diagnostics") {
        return RiskClass::SensitiveRead;
    }
    match family {
        CapabilityFamily::Filesystem
        | CapabilityFamily::Command
        | CapabilityFamily::Process
        | CapabilityFamily::Review => RiskClass::Execution,
        CapabilityFamily::Threads
        | CapabilityFamily::Realtime
        | CapabilityFamily::WindowsSandbox => RiskClass::Deferred,
        CapabilityFamily::Other => RiskClass::Unsupported,
        _ => RiskClass::Mutation,
    }
}

fn family_for(method: &str) -> CapabilityFamily {
    let prefix = method.split('/').next().unwrap_or(method);
    match prefix {
        "initialize" | "server" | "serverRequest" | "experimentalFeature" => {
            CapabilityFamily::Runtime
        }
        "account" => CapabilityFamily::Account,
        "config" | "configRequirements" => CapabilityFamily::Config,
        "model" | "modelProvider" | "collaborationMode" | "permissionProfile" => {
            CapabilityFamily::Models
        }
        "mcpServer" | "mcpServerStatus" => CapabilityFamily::Mcp,
        "skills" => CapabilityFamily::Skills,
        "hook" | "hooks" => CapabilityFamily::Hooks,
        "plugin" => CapabilityFamily::Plugins,
        "marketplace" => CapabilityFamily::Marketplace,
        "app" => CapabilityFamily::Apps,
        "externalAgentConfig" => CapabilityFamily::ExternalAgentConfig,
        "fs" | "fuzzyFileSearch" => CapabilityFamily::Filesystem,
        "command" => CapabilityFamily::Command,
        "process" => CapabilityFamily::Process,
        "project" | "environment" => CapabilityFamily::Project,
        "remoteControl" => CapabilityFamily::RemoteControl,
        "thread" | "threadSection" | "turn" | "item" => {
            if method.contains("realtime") {
                CapabilityFamily::Realtime
            } else {
                CapabilityFamily::Threads
            }
        }
        "review" | "autoApprovalReview" | "guardianWarning" => CapabilityFamily::Review,
        "windows" | "windowsSandbox" => CapabilityFamily::WindowsSandbox,
        "applyPatchApproval" | "execCommandApproval" => CapabilityFamily::Approval,
        "attestation" | "currentTime" | "feedback" | "memory" | "mock" | "configWarning"
        | "deprecationNotice" | "error" | "warning" => CapabilityFamily::System,
        _ => CapabilityFamily::Other,
    }
}

fn is_approval(method: &str) -> bool {
    method.ends_with("requestApproval")
        || matches!(
            method,
            "applyPatchApproval"
                | "execCommandApproval"
                | "item/tool/requestUserInput"
                | "mcpServer/elicitation/request"
        )
}

#[cfg(test)]
#[path = "capabilities_tests.rs"]
mod tests;
