//! Typed control-plane requests and secret-safe projections.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

mod validation;
pub use validation::validate_mutation_params;
pub(crate) use validation::{contains_sensitive_url, is_sensitive_identifier};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ControlAction {
    AccountRead,
    AccountLoginStart,
    AccountLoginCancel,
    AccountLogout,
    RateLimitsRead,
    AccountUsageRead,
    AccountWorkspaceMessagesRead,
    AccountRateLimitResetCreditConsume,
    AccountBedrockDiscover,
    AccountBedrockSetup,
    ModelsList,
    ModelProviderCapabilitiesRead,
    CollaborationModesList,
    PermissionProfilesList,
    ConfigRead,
    ConfigRequirementsRead,
    ConfigValueWrite,
    ConfigBatchWrite,
    McpServersList,
    McpServerResourceRead,
    McpServerToolCall,
    McpServerEventStreamStart,
    McpServerEventStreamStop,
    McpServerReload,
    McpServerOauthLogin,
    PluginsList,
    PluginsInstalled,
    PluginSearch,
    PluginRead,
    PluginSkillRead,
    PluginShareList,
    PluginShareCheckout,
    PluginShareSave,
    PluginShareDelete,
    PluginShareUpdateTargets,
    PluginInstall,
    PluginUninstall,
    MarketplaceAdd,
    MarketplaceRemove,
    MarketplaceUpgrade,
    SkillsList,
    SkillsExtraRootsSet,
    SkillConfigWrite,
    ExternalAgentConfigDetect,
    ExternalAgentConfigImport,
    ExternalAgentConfigImportReadHistories,
    ExternalAgentConfigImportRecordHistory,
    HooksList,
    AppsList,
    AppsInstalled,
    AppRead,
    ExperimentalFeaturesList,
    ExperimentalFeatureEnablementSet,
}

/// Write-only actions accepted by the approved mutation workflow.
/// Keeping this separate from [`ControlAction`] prevents callers from preparing
/// or executing a read method through the mutation endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MutationAction {
    AccountLoginStart,
    AccountLoginCancel,
    AccountLogout,
    AccountRateLimitResetCreditConsume,
    AccountBedrockSetup,
    ConfigValueWrite,
    ConfigBatchWrite,
    McpServerReload,
    McpServerOauthLogin,
    McpServerToolCall,
    McpServerEventStreamStart,
    McpServerEventStreamStop,
    PluginInstall,
    PluginUninstall,
    PluginShareCheckout,
    PluginShareSave,
    PluginShareDelete,
    PluginShareUpdateTargets,
    MarketplaceAdd,
    MarketplaceRemove,
    MarketplaceUpgrade,
    SkillConfigWrite,
    SkillsExtraRootsSet,
    ExternalAgentConfigImport,
    ExternalAgentConfigImportRecordHistory,
    ExperimentalFeatureEnablementSet,
}

impl MutationAction {
    pub const ALL: [Self; 26] = [
        Self::AccountLoginStart,
        Self::AccountLoginCancel,
        Self::AccountLogout,
        Self::AccountRateLimitResetCreditConsume,
        Self::AccountBedrockSetup,
        Self::ConfigValueWrite,
        Self::ConfigBatchWrite,
        Self::McpServerReload,
        Self::McpServerOauthLogin,
        Self::McpServerToolCall,
        Self::McpServerEventStreamStart,
        Self::McpServerEventStreamStop,
        Self::PluginInstall,
        Self::PluginUninstall,
        Self::PluginShareCheckout,
        Self::PluginShareSave,
        Self::PluginShareDelete,
        Self::PluginShareUpdateTargets,
        Self::MarketplaceAdd,
        Self::MarketplaceRemove,
        Self::MarketplaceUpgrade,
        Self::SkillConfigWrite,
        Self::SkillsExtraRootsSet,
        Self::ExternalAgentConfigImport,
        Self::ExternalAgentConfigImportRecordHistory,
        Self::ExperimentalFeatureEnablementSet,
    ];

    pub const fn method(self) -> &'static str {
        self.control_action().method()
    }

    pub const fn control_action(self) -> ControlAction {
        match self {
            Self::AccountLoginStart => ControlAction::AccountLoginStart,
            Self::AccountLoginCancel => ControlAction::AccountLoginCancel,
            Self::AccountLogout => ControlAction::AccountLogout,
            Self::AccountRateLimitResetCreditConsume => {
                ControlAction::AccountRateLimitResetCreditConsume
            }
            Self::AccountBedrockSetup => ControlAction::AccountBedrockSetup,
            Self::ConfigValueWrite => ControlAction::ConfigValueWrite,
            Self::ConfigBatchWrite => ControlAction::ConfigBatchWrite,
            Self::McpServerReload => ControlAction::McpServerReload,
            Self::McpServerOauthLogin => ControlAction::McpServerOauthLogin,
            Self::McpServerToolCall => ControlAction::McpServerToolCall,
            Self::McpServerEventStreamStart => ControlAction::McpServerEventStreamStart,
            Self::McpServerEventStreamStop => ControlAction::McpServerEventStreamStop,
            Self::PluginInstall => ControlAction::PluginInstall,
            Self::PluginUninstall => ControlAction::PluginUninstall,
            Self::PluginShareCheckout => ControlAction::PluginShareCheckout,
            Self::PluginShareSave => ControlAction::PluginShareSave,
            Self::PluginShareDelete => ControlAction::PluginShareDelete,
            Self::PluginShareUpdateTargets => ControlAction::PluginShareUpdateTargets,
            Self::MarketplaceAdd => ControlAction::MarketplaceAdd,
            Self::MarketplaceRemove => ControlAction::MarketplaceRemove,
            Self::MarketplaceUpgrade => ControlAction::MarketplaceUpgrade,
            Self::SkillConfigWrite => ControlAction::SkillConfigWrite,
            Self::SkillsExtraRootsSet => ControlAction::SkillsExtraRootsSet,
            Self::ExternalAgentConfigImport => ControlAction::ExternalAgentConfigImport,
            Self::ExternalAgentConfigImportRecordHistory => {
                ControlAction::ExternalAgentConfigImportRecordHistory
            }
            Self::ExperimentalFeatureEnablementSet => {
                ControlAction::ExperimentalFeatureEnablementSet
            }
        }
    }
}

impl TryFrom<&str> for MutationAction {
    type Error = String;

    fn try_from(method: &str) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|action| action.method() == method)
            .ok_or_else(|| format!("unsupported Codex control mutation: {method}"))
    }
}

impl From<MutationAction> for ControlAction {
    fn from(action: MutationAction) -> Self {
        action.control_action()
    }
}

impl ControlAction {
    pub const fn method(&self) -> &'static str {
        match self {
            Self::AccountRead => "account/read",
            Self::AccountLoginStart => "account/login/start",
            Self::AccountLoginCancel => "account/login/cancel",
            Self::AccountLogout => "account/logout",
            Self::RateLimitsRead => "account/rateLimits/read",
            Self::AccountUsageRead => "account/usage/read",
            Self::AccountWorkspaceMessagesRead => "account/workspaceMessages/read",
            Self::AccountRateLimitResetCreditConsume => "account/rateLimitResetCredit/consume",
            Self::AccountBedrockDiscover => "account/bedrock/discover",
            Self::AccountBedrockSetup => "account/bedrock/setup",
            Self::ModelsList => "model/list",
            Self::ModelProviderCapabilitiesRead => "modelProvider/capabilities/read",
            Self::CollaborationModesList => "collaborationMode/list",
            Self::PermissionProfilesList => "permissionProfile/list",
            Self::ConfigRead => "config/read",
            Self::ConfigRequirementsRead => "configRequirements/read",
            Self::ConfigValueWrite => "config/value/write",
            Self::ConfigBatchWrite => "config/batchWrite",
            Self::McpServersList => "mcpServerStatus/list",
            Self::McpServerResourceRead => "mcpServer/resource/read",
            Self::McpServerToolCall => "mcpServer/tool/call",
            Self::McpServerEventStreamStart => "mcpServer/event/stream/start",
            Self::McpServerEventStreamStop => "mcpServer/event/stream/stop",
            Self::McpServerReload => "config/mcpServer/reload",
            Self::McpServerOauthLogin => "mcpServer/oauth/login",
            Self::PluginsList => "plugin/list",
            Self::PluginsInstalled => "plugin/installed",
            Self::PluginSearch => "plugin/search",
            Self::PluginRead => "plugin/read",
            Self::PluginSkillRead => "plugin/skill/read",
            Self::PluginShareList => "plugin/share/list",
            Self::PluginShareCheckout => "plugin/share/checkout",
            Self::PluginShareSave => "plugin/share/save",
            Self::PluginShareDelete => "plugin/share/delete",
            Self::PluginShareUpdateTargets => "plugin/share/updateTargets",
            Self::PluginInstall => "plugin/install",
            Self::PluginUninstall => "plugin/uninstall",
            Self::MarketplaceAdd => "marketplace/add",
            Self::MarketplaceRemove => "marketplace/remove",
            Self::MarketplaceUpgrade => "marketplace/upgrade",
            Self::SkillsList => "skills/list",
            Self::SkillsExtraRootsSet => "skills/extraRoots/set",
            Self::SkillConfigWrite => "skills/config/write",
            Self::ExternalAgentConfigDetect => "externalAgentConfig/detect",
            Self::ExternalAgentConfigImport => "externalAgentConfig/import",
            Self::ExternalAgentConfigImportReadHistories => {
                "externalAgentConfig/import/readHistories"
            }
            Self::ExternalAgentConfigImportRecordHistory => {
                "externalAgentConfig/import/recordHistory"
            }
            Self::HooksList => "hooks/list",
            Self::AppsList => "app/list",
            Self::AppsInstalled => "app/installed",
            Self::AppRead => "app/read",
            Self::ExperimentalFeaturesList => "experimentalFeature/list",
            Self::ExperimentalFeatureEnablementSet => "experimentalFeature/enablement/set",
        }
    }

    pub const fn is_mutation(&self) -> bool {
        !matches!(
            self,
            Self::AccountRead
                | Self::RateLimitsRead
                | Self::AccountUsageRead
                | Self::AccountWorkspaceMessagesRead
                | Self::AccountBedrockDiscover
                | Self::ModelsList
                | Self::ModelProviderCapabilitiesRead
                | Self::CollaborationModesList
                | Self::PermissionProfilesList
                | Self::ConfigRead
                | Self::ConfigRequirementsRead
                | Self::McpServersList
                | Self::McpServerResourceRead
                | Self::PluginsList
                | Self::PluginsInstalled
                | Self::PluginSearch
                | Self::PluginRead
                | Self::PluginSkillRead
                | Self::PluginShareList
                | Self::SkillsList
                | Self::ExternalAgentConfigDetect
                | Self::ExternalAgentConfigImportReadHistories
                | Self::HooksList
                | Self::AppsList
                | Self::AppsInstalled
                | Self::AppRead
                | Self::ExperimentalFeaturesList
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSummary {
    pub signed_in: bool,
    pub account_type: Option<String>,
    pub email_hint: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WritePolicy {
    pub account: bool,
    pub config: bool,
    pub mcp: bool,
    pub plugins: bool,
    pub skills: bool,
    pub imports: bool,
}

impl WritePolicy {
    pub const fn deny_all() -> Self {
        Self {
            account: false,
            config: false,
            mcp: false,
            plugins: false,
            skills: false,
            imports: false,
        }
    }

    pub fn authorize(&self, action: &ControlAction) -> Result<(), String> {
        if !action.is_mutation() {
            return Ok(());
        }
        let allowed = match action {
            ControlAction::AccountLoginStart
            | ControlAction::AccountLoginCancel
            | ControlAction::AccountLogout
            | ControlAction::AccountRateLimitResetCreditConsume
            | ControlAction::AccountBedrockSetup => self.account,
            ControlAction::ConfigValueWrite | ControlAction::ConfigBatchWrite => self.config,
            ControlAction::McpServerReload
            | ControlAction::McpServerOauthLogin
            | ControlAction::McpServerToolCall
            | ControlAction::McpServerEventStreamStart
            | ControlAction::McpServerEventStreamStop => self.mcp,
            ControlAction::PluginInstall
            | ControlAction::PluginUninstall
            | ControlAction::PluginShareCheckout
            | ControlAction::PluginShareSave
            | ControlAction::PluginShareDelete
            | ControlAction::PluginShareUpdateTargets
            | ControlAction::MarketplaceAdd
            | ControlAction::MarketplaceRemove
            | ControlAction::MarketplaceUpgrade => self.plugins,
            ControlAction::SkillConfigWrite | ControlAction::SkillsExtraRootsSet => self.skills,
            ControlAction::ExternalAgentConfigImport
            | ControlAction::ExternalAgentConfigImportRecordHistory => self.imports,
            ControlAction::ExperimentalFeatureEnablementSet => self.config,
            _ => false,
        };
        allowed
            .then_some(())
            .ok_or_else(|| format!("{} is denied by Codex control policy", action.method()))
    }
}

pub fn state_revision(value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

/// Decode an account response without retaining tokens or raw auth payloads.
pub fn account_summary(result: &Value) -> AccountSummary {
    let account = result.get("account").unwrap_or(result);
    AccountSummary {
        signed_in: !account.is_null() && account.as_object().is_some_and(|value| !value.is_empty()),
        account_type: account
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_owned),
        email_hint: account.get("email").and_then(Value::as_str).map(mask_email),
        plan: account
            .get("planType")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

fn mask_email(email: &str) -> String {
    let Some((local, domain)) = email.split_once('@') else {
        return "***".to_string();
    };
    let initial = local.chars().next().unwrap_or('*');
    format!("{initial}***@{domain}")
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
