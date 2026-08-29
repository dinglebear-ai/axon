//! Typed control-plane requests and secret-safe projections.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
    MarketplaceAdd,
    MarketplaceRemove,
    MarketplaceUpgrade,
    SkillsList,
    SkillConfigWrite,
    ExternalAgentConfigDetect,
    ExternalAgentConfigImport,
    HooksList,
    AppsList,
}

/// Write-only actions accepted by the approved mutation workflow.
///
/// Keeping this separate from [`ControlAction`] prevents callers from preparing
/// or executing a read method through the mutation endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MutationAction {
    AccountLoginStart,
    AccountLoginCancel,
    AccountLogout,
    ConfigValueWrite,
    ConfigBatchWrite,
    McpServerReload,
    McpServerOauthLogin,
    PluginInstall,
    PluginUninstall,
    MarketplaceAdd,
    MarketplaceRemove,
    MarketplaceUpgrade,
    SkillConfigWrite,
    ExternalAgentConfigImport,
}

impl MutationAction {
    pub const fn method(self) -> &'static str {
        match self {
            Self::AccountLoginStart => "account/login/start",
            Self::AccountLoginCancel => "account/login/cancel",
            Self::AccountLogout => "account/logout",
            Self::ConfigValueWrite => "config/value/write",
            Self::ConfigBatchWrite => "config/batchWrite",
            Self::McpServerReload => "config/mcpServer/reload",
            Self::McpServerOauthLogin => "mcpServer/oauth/login",
            Self::PluginInstall => "plugin/install",
            Self::PluginUninstall => "plugin/uninstall",
            Self::MarketplaceAdd => "marketplace/add",
            Self::MarketplaceRemove => "marketplace/remove",
            Self::MarketplaceUpgrade => "marketplace/upgrade",
            Self::SkillConfigWrite => "skills/config/write",
            Self::ExternalAgentConfigImport => "externalAgentConfig/import",
        }
    }
}

impl From<MutationAction> for ControlAction {
    fn from(action: MutationAction) -> Self {
        match action {
            MutationAction::AccountLoginStart => Self::AccountLoginStart,
            MutationAction::AccountLoginCancel => Self::AccountLoginCancel,
            MutationAction::AccountLogout => Self::AccountLogout,
            MutationAction::ConfigValueWrite => Self::ConfigValueWrite,
            MutationAction::ConfigBatchWrite => Self::ConfigBatchWrite,
            MutationAction::McpServerReload => Self::McpServerReload,
            MutationAction::McpServerOauthLogin => Self::McpServerOauthLogin,
            MutationAction::PluginInstall => Self::PluginInstall,
            MutationAction::PluginUninstall => Self::PluginUninstall,
            MutationAction::MarketplaceAdd => Self::MarketplaceAdd,
            MutationAction::MarketplaceRemove => Self::MarketplaceRemove,
            MutationAction::MarketplaceUpgrade => Self::MarketplaceUpgrade,
            MutationAction::SkillConfigWrite => Self::SkillConfigWrite,
            MutationAction::ExternalAgentConfigImport => Self::ExternalAgentConfigImport,
        }
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
            Self::MarketplaceAdd => "marketplace/add",
            Self::MarketplaceRemove => "marketplace/remove",
            Self::MarketplaceUpgrade => "marketplace/upgrade",
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
            ControlAction::PluginInstall
            | ControlAction::PluginUninstall
            | ControlAction::MarketplaceAdd
            | ControlAction::MarketplaceRemove
            | ControlAction::MarketplaceUpgrade => self.plugins,
            ControlAction::SkillConfigWrite => self.skills,
            ControlAction::ExternalAgentConfigImport => self.imports,
            _ => false,
        };
        allowed
            .then_some(())
            .ok_or_else(|| format!("{} is denied by Codex control policy", action.method()))
    }
}

pub fn validate_mutation_params(action: &ControlAction, params: &Value) -> Result<(), String> {
    let encoded = serde_json::to_vec(params).map_err(|error| error.to_string())?;
    if encoded.len() > 64 * 1024 {
        return Err("Codex mutation parameters exceed 64 KiB".to_string());
    }
    if !params.is_object() {
        return Err("Codex mutation parameters must be a JSON object".to_string());
    }
    reject_plaintext_secrets(params)?;
    if matches!(
        action,
        ControlAction::PluginInstall
            | ControlAction::MarketplaceAdd
            | ControlAction::ExternalAgentConfigImport
    ) {
        let source = params
            .get("source")
            .and_then(Value::as_str)
            .ok_or("artifact mutation requires a pinned HTTPS source")?;
        if !source.starts_with("https://") {
            return Err(
                "artifact sources must use HTTPS; local and file sources are disabled".to_string(),
            );
        }
        let digest = params
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or("artifact source requires an immutable sha256 digest")?;
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("artifact sha256 must be exactly 64 hexadecimal characters".to_string());
        }
    }
    Ok(())
}

pub fn state_revision(value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn reject_plaintext_secrets(value: &Value) -> Result<(), String> {
    match value {
        Value::Object(values) => {
            for (key, value) in values {
                let lowered = key.to_ascii_lowercase();
                let secret_key = ["token", "secret", "password", "authorization", "cookie"]
                    .iter()
                    .any(|needle| lowered.contains(needle));
                if secret_key && value.as_str().is_some_and(|text| !text.starts_with("env:")) {
                    return Err(format!("{key} must use an env: secret reference"));
                }
                reject_plaintext_secrets(value)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                reject_plaintext_secrets(value)?;
            }
        }
        _ => {}
    }
    Ok(())
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
