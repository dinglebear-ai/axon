//! Typed control-plane requests and secret-safe projections.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlAction {
    AccountRead,
    AccountLoginStart,
    AccountLoginCancel,
    AccountLogout,
    RateLimitsRead,
    ModelsList,
    ModelProviderCapabilitiesRead,
    ConfigRead,
    ConfigValueWrite,
    ConfigBatchWrite,
    McpServersList,
    McpServerReload,
    McpServerOauthLogin,
    PluginsList,
    PluginRead,
    PluginInstall,
    PluginUninstall,
    SkillsList,
    SkillConfigWrite,
    ExternalAgentConfigDetect,
    ExternalAgentConfigImport,
    HooksList,
    AppsList,
}

impl ControlAction {
    pub const fn method(&self) -> &'static str {
        match self {
            Self::AccountRead => "account/read",
            Self::AccountLoginStart => "account/login/start",
            Self::AccountLoginCancel => "account/login/cancel",
            Self::AccountLogout => "account/logout",
            Self::RateLimitsRead => "account/rateLimits/read",
            Self::ModelsList => "model/list",
            Self::ModelProviderCapabilitiesRead => "modelProvider/capabilities/read",
            Self::ConfigRead => "config/read",
            Self::ConfigValueWrite => "config/value/write",
            Self::ConfigBatchWrite => "config/batchWrite",
            Self::McpServersList => "mcpServerStatus/list",
            Self::McpServerReload => "config/mcpServer/reload",
            Self::McpServerOauthLogin => "mcpServer/oauth/login",
            Self::PluginsList => "plugin/list",
            Self::PluginRead => "plugin/read",
            Self::PluginInstall => "plugin/install",
            Self::PluginUninstall => "plugin/uninstall",
            Self::SkillsList => "skills/list",
            Self::SkillConfigWrite => "skills/config/write",
            Self::ExternalAgentConfigDetect => "externalAgentConfig/detect",
            Self::ExternalAgentConfigImport => "externalAgentConfig/import",
            Self::HooksList => "hooks/list",
            Self::AppsList => "app/list",
        }
    }

    pub const fn is_mutation(&self) -> bool {
        !matches!(
            self,
            Self::AccountRead
                | Self::RateLimitsRead
                | Self::ModelsList
                | Self::ModelProviderCapabilitiesRead
                | Self::ConfigRead
                | Self::McpServersList
                | Self::PluginsList
                | Self::PluginRead
                | Self::SkillsList
                | Self::ExternalAgentConfigDetect
                | Self::HooksList
                | Self::AppsList
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    pub revision: String,
    pub persisted: Value,
    pub active: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigWrite {
    pub key_path: String,
    pub value: Value,
    pub expected_revision: String,
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
            | ControlAction::AccountLogout => self.account,
            ControlAction::ConfigValueWrite | ControlAction::ConfigBatchWrite => self.config,
            ControlAction::McpServerReload | ControlAction::McpServerOauthLogin => self.mcp,
            ControlAction::PluginInstall | ControlAction::PluginUninstall => self.plugins,
            ControlAction::SkillConfigWrite => self.skills,
            ControlAction::ExternalAgentConfigImport => self.imports,
            _ => false,
        };
        allowed
            .then_some(())
            .ok_or_else(|| format!("{} is denied by Codex control policy", action.method()))
    }
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
