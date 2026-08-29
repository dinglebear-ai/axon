use super::AxonMcpServer;
use super::common::{invalid_params, logged_internal_error};
use crate::schema::{
    AxonToolResponse, CodexMutationAction, CodexRequest, CodexResource, CodexSubaction,
};
use axon_services::codex_control::{ControlAction, EventCursor, MutationAction, OperationIntent};
use rmcp::ErrorData;
use serde_json::json;

impl AxonMcpServer {
    pub(super) async fn handle_codex(
        &self,
        req: CodexRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let service = self
            .codex_control_service()
            .await
            .map_err(|error| invalid_params(format!("codex.initialize: {error}")))?;
        let subaction = format!("{:?}", req.subaction).to_ascii_lowercase();
        let caller = super::common::CURRENT_CODEX_CALLER
            .try_with(Clone::clone)
            .unwrap_or(super::common::CodexCaller {
                actor: "trusted-loopback".to_string(),
                scopes: "local-trusted".to_string(),
            });
        let data = match req.subaction {
            CodexSubaction::Snapshot => {
                serde_json::to_value(service.snapshot().await.map_err(codex_error)?)
                    .map_err(|error| logged_internal_error("codex.snapshot", &error))?
            }
            CodexSubaction::Resource => {
                let resource = required(req.resource, "resource")?;
                let action = resource_action(&resource)?;
                service
                    .read(action, req.params.unwrap_or_else(|| json!({})))
                    .await
                    .map_err(codex_error)?
            }
            CodexSubaction::Events => {
                let cursor = match (req.cursor_boot_id, req.after_sequence) {
                    (Some(boot_id), Some(sequence)) => Some(EventCursor { boot_id, sequence }),
                    _ => None,
                };
                serde_json::to_value(
                    service
                        .events_after(cursor, req.limit.unwrap_or(100))
                        .await
                        .map_err(codex_error)?,
                )
                .map_err(|error| logged_internal_error("codex.events", &error))?
            }
            CodexSubaction::Operations => {
                serde_json::to_value(service.unfinished_operations().map_err(codex_error)?)
                    .map_err(|error| logged_internal_error("codex.operations", &error))?
            }
            CodexSubaction::Prepare => {
                let mutation = mutation_action(required(req.mutation_action, "mutation_action")?)?;
                let params = req
                    .params
                    .ok_or_else(|| invalid_params("codex prepare requires params"))?;
                let intent = OperationIntent {
                    actor: caller.actor.clone(),
                    scope: caller.scopes,
                    method: mutation.method().to_string(),
                    target_home_identity: String::new(),
                    runtime_boot_id: 0,
                    policy_version: String::new(),
                    expected_revision: None,
                    idempotency_key: required(req.idempotency_key, "idempotency_key")?,
                    redacted_request: params,
                };
                serde_json::to_value(
                    service
                        .create_operation(mutation, &intent)
                        .await
                        .map_err(codex_error)?,
                )
                .map_err(|error| logged_internal_error("codex.prepare", &error))?
            }
            CodexSubaction::Approve => json!({
                "approval_capability": service.approve_operation(required(req.operation_id, "operation_id")?, &caller.actor).map_err(codex_error)?
            }),
            CodexSubaction::Execute => {
                let mutation = mutation_action(required(req.mutation_action, "mutation_action")?)?;
                service
                    .execute_operation(
                        required(req.operation_id, "operation_id")?,
                        &required(req.capability, "capability")?,
                        mutation,
                        req.params
                            .ok_or_else(|| invalid_params("codex execute requires params"))?,
                    )
                    .await
                    .map_err(codex_error)?
            }
            CodexSubaction::Cancel => {
                let operation_id = required(req.operation_id, "operation_id")?;
                service
                    .cancel_operation(operation_id)
                    .map_err(codex_error)?;
                json!({"operation_id": operation_id, "cancelled": true})
            }
            CodexSubaction::Reconcile => reconcile_operation(&service, &req).await?,
            CodexSubaction::Respond => {
                service
                    .respond_to_server_request(
                        required(req.boot_id, "boot_id")?,
                        required(req.request_id, "request_id")?,
                        required(req.approved, "approved")?,
                        req.params,
                    )
                    .await
                    .map_err(codex_error)?;
                json!({"responded": true})
            }
        };
        Ok(AxonToolResponse::ok("codex", &subaction, data))
    }
}

async fn reconcile_operation(
    service: &axon_services::codex_control::CodexControlService,
    req: &CodexRequest,
) -> Result<serde_json::Value, ErrorData> {
    let operation_id = required(req.operation_id, "operation_id")?;
    if req.without_replay.unwrap_or(false) {
        service
            .resolve_recovery_without_replay(
                operation_id,
                required(req.effect_applied, "effect_applied")?,
                req.disposition_note
                    .as_deref()
                    .ok_or_else(|| invalid_params("codex request requires disposition_note"))?,
            )
            .map_err(codex_error)?;
    } else {
        service
            .resolve_recovery(operation_id)
            .await
            .map_err(codex_error)?;
    }
    Ok(
        json!({"operation_id": operation_id, "reconciled": true, "without_replay": req.without_replay.unwrap_or(false), "effect_applied": req.effect_applied}),
    )
}

fn required<T>(value: Option<T>, name: &str) -> Result<T, ErrorData> {
    value.ok_or_else(|| invalid_params(format!("codex request requires {name}")))
}

fn mutation_action(value: CodexMutationAction) -> Result<MutationAction, ErrorData> {
    serde_json::from_value(
        serde_json::to_value(value)
            .map_err(|error| invalid_params(format!("invalid Codex mutation action: {error}")))?,
    )
    .map_err(|error| invalid_params(format!("invalid Codex mutation action: {error}")))
}

fn resource_action(resource: &CodexResource) -> Result<ControlAction, ErrorData> {
    match resource {
        CodexResource::Account => Ok(ControlAction::AccountRead),
        CodexResource::RateLimits => Ok(ControlAction::RateLimitsRead),
        CodexResource::AccountUsage => Ok(ControlAction::AccountUsageRead),
        CodexResource::AccountWorkspaceMessages => Ok(ControlAction::AccountWorkspaceMessagesRead),
        CodexResource::AccountBedrock => Ok(ControlAction::AccountBedrockDiscover),
        CodexResource::Models => Ok(ControlAction::ModelsList),
        CodexResource::ModelProviderCapabilities => {
            Ok(ControlAction::ModelProviderCapabilitiesRead)
        }
        CodexResource::CollaborationModes => Ok(ControlAction::CollaborationModesList),
        CodexResource::PermissionProfiles => Ok(ControlAction::PermissionProfilesList),
        CodexResource::Config => Ok(ControlAction::ConfigRead),
        CodexResource::ConfigRequirements => Ok(ControlAction::ConfigRequirementsRead),
        CodexResource::McpServers => Ok(ControlAction::McpServersList),
        CodexResource::McpResource => Ok(ControlAction::McpServerResourceRead),
        CodexResource::Plugins => Ok(ControlAction::PluginsList),
        CodexResource::PluginsInstalled => Ok(ControlAction::PluginsInstalled),
        CodexResource::PluginSearch => Ok(ControlAction::PluginSearch),
        CodexResource::Plugin => Ok(ControlAction::PluginRead),
        CodexResource::PluginSkill => Ok(ControlAction::PluginSkillRead),
        CodexResource::PluginShares => Ok(ControlAction::PluginShareList),
        CodexResource::Skills => Ok(ControlAction::SkillsList),
        CodexResource::ExternalAgentConfig => Ok(ControlAction::ExternalAgentConfigDetect),
        CodexResource::ExternalAgentImportHistories => {
            Ok(ControlAction::ExternalAgentConfigImportReadHistories)
        }
        CodexResource::Hooks => Ok(ControlAction::HooksList),
        CodexResource::Apps => Ok(ControlAction::AppsList),
        CodexResource::AppsInstalled => Ok(ControlAction::AppsInstalled),
        CodexResource::App => Ok(ControlAction::AppRead),
        CodexResource::ExperimentalFeatures => Ok(ControlAction::ExperimentalFeaturesList),
    }
}

fn codex_error(error: String) -> ErrorData {
    invalid_params(format!("Codex control request failed: {error}"))
}

#[cfg(test)]
#[path = "handlers_codex_tests.rs"]
mod tests;
