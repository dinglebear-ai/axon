use super::{ControlAction, ControlTransport, MutationAction};
use axon_codex::api::state_revision;
use axon_codex::events::sanitize_value;
use serde_json::{Value, json};

pub(super) async fn canonical_revision(
    action: &ControlAction,
    transport: &ControlTransport,
) -> Result<String, String> {
    Ok(canonical_state(action, transport).await?.revision)
}

pub(super) struct CanonicalState {
    pub(super) value: Value,
    pub(super) revision: String,
}

pub(super) async fn canonical_state(
    action: &ControlAction,
    transport: &ControlTransport,
) -> Result<CanonicalState, String> {
    let (method, params) = match action {
        ControlAction::AccountLoginStart
        | ControlAction::AccountLoginCancel
        | ControlAction::AccountLogout
        | ControlAction::AccountRateLimitResetCreditConsume => (
            ControlAction::AccountRead.method(),
            json!({"refreshToken":false}),
        ),
        ControlAction::AccountBedrockSetup => {
            (ControlAction::AccountBedrockDiscover.method(), json!({}))
        }
        ControlAction::ExperimentalFeatureEnablementSet => {
            (ControlAction::ExperimentalFeaturesList.method(), json!({}))
        }
        ControlAction::McpServerReload | ControlAction::McpServerOauthLogin => {
            (ControlAction::McpServersList.method(), json!({}))
        }
        ControlAction::McpServerToolCall
        | ControlAction::McpServerEventStreamStart
        | ControlAction::McpServerEventStreamStop => {
            (ControlAction::McpServersList.method(), json!({}))
        }
        ControlAction::PluginInstall
        | ControlAction::PluginUninstall
        | ControlAction::MarketplaceAdd
        | ControlAction::MarketplaceRemove
        | ControlAction::MarketplaceUpgrade
        | ControlAction::PluginShareCheckout
        | ControlAction::PluginShareSave
        | ControlAction::PluginShareDelete
        | ControlAction::PluginShareUpdateTargets => {
            (ControlAction::PluginsList.method(), json!({}))
        }
        ControlAction::SkillConfigWrite
        | ControlAction::SkillsExtraRootsSet
        | ControlAction::ExternalAgentConfigImport
        | ControlAction::ExternalAgentConfigImportRecordHistory => {
            (ControlAction::SkillsList.method(), json!({}))
        }
        _ => (
            ControlAction::ConfigRead.method(),
            json!({"includeLayers":true}),
        ),
    };
    let value = sanitize_value(transport.request(method, params).await?);
    let revision = state_revision(&value)?;
    Ok(CanonicalState { value, revision })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CompletionStrategy {
    CanonicalReadback,
    ResponseAcknowledged,
}

pub(super) fn completion_strategy(action: &ControlAction) -> CompletionStrategy {
    if matches!(
        action,
        ControlAction::AccountLoginStart
            | ControlAction::AccountLoginCancel
            | ControlAction::AccountRateLimitResetCreditConsume
            | ControlAction::AccountBedrockSetup
            | ControlAction::McpServerReload
            | ControlAction::McpServerOauthLogin
            | ControlAction::McpServerToolCall
            | ControlAction::McpServerEventStreamStart
            | ControlAction::McpServerEventStreamStop
            | ControlAction::PluginShareCheckout
            | ControlAction::PluginShareSave
            | ControlAction::PluginShareDelete
            | ControlAction::PluginShareUpdateTargets
            | ControlAction::MarketplaceUpgrade
            | ControlAction::SkillsExtraRootsSet
            | ControlAction::ExternalAgentConfigImportRecordHistory
    ) {
        CompletionStrategy::ResponseAcknowledged
    } else {
        CompletionStrategy::CanonicalReadback
    }
}

pub(super) fn action_from_method(method: &str) -> Result<ControlAction, String> {
    MutationAction::try_from(method).map(ControlAction::from)
}
