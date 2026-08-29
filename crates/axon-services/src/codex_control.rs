//! Services-first facade for the dedicated Codex app-server control plane.

pub use axon_codex::api::{ControlAction, MutationAction};
use axon_codex::api::{WritePolicy, account_summary, state_revision, validate_mutation_params};
use axon_codex::control::{ControlConfig, ControlRuntime, ControlStatus, home_identity};
use axon_codex::events::sanitize_value;
pub use axon_codex::events::{EventCursor, RecordedEvent};
use axon_codex::operations::OperationStore;
pub use axon_codex::operations::{ControlOperation, OperationIntent, OperationPhase};
use axon_codex::protocol::RuntimeEpoch;
use axon_codex::transport::ControlTransport;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::Mutex;
use utoipa::ToSchema;

mod postconditions;
use postconditions::{EffectProof, verify_intended_effect};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CodexControlSnapshot {
    pub status: ControlStatus,
    pub account: Value,
    pub models: Value,
    pub config: Value,
    pub mcp_servers: Value,
    pub plugins: Value,
    pub skills: Value,
    pub hooks: Value,
    pub apps: Value,
    pub pending_server_requests: Vec<RecordedEvent>,
}

pub struct CodexControlService {
    config: ControlConfig,
    runtime: Arc<ControlRuntime>,
    transport: Mutex<Option<Arc<ControlTransport>>>,
    operations: OperationStore,
    policy: WritePolicy,
    home_identity: String,
    runtime_boot_id: AtomicU64,
    policy_version: String,
}

impl CodexControlService {
    pub fn from_config(cfg: &axon_core::config::Config) -> Result<Option<Arc<Self>>, String> {
        if !cfg.codex_control_enabled {
            return Ok(None);
        }
        let home = cfg.codex_control_home.clone().ok_or_else(|| {
            "AXON_CODEX_CONTROL_HOME is required when Codex control is enabled".to_string()
        })?;
        let control = ControlConfig {
            enabled: true,
            codex_binary: std::path::PathBuf::from(&cfg.codex_cmd),
            control_home: home,
            request_timeout: std::time::Duration::from_secs(
                cfg.codex_control_request_timeout_secs.max(1),
            ),
            read_concurrency: cfg.codex_control_read_concurrency.max(1),
            max_restart_backoff: std::time::Duration::from_secs(60),
        };
        let policy = WritePolicy {
            account: cfg.codex_control_account_writes,
            config: cfg.codex_control_config_writes,
            mcp: cfg.codex_control_mcp_writes,
            plugins: cfg.codex_control_plugin_writes,
            skills: cfg.codex_control_skill_writes,
            imports: cfg.codex_control_skill_writes,
        };
        let database = cfg.sqlite_path.with_file_name("codex-control.db");
        Self::new(control, policy, &database)
            .map(Arc::new)
            .map(Some)
    }

    pub fn new(
        config: ControlConfig,
        policy: WritePolicy,
        database: &Path,
    ) -> Result<Self, String> {
        let home_identity = home_identity(&config.control_home)?;
        let policy_version = format!(
            "v1:a{}c{}m{}p{}s{}i{}",
            policy.account as u8,
            policy.config as u8,
            policy.mcp as u8,
            policy.plugins as u8,
            policy.skills as u8,
            policy.imports as u8
        );
        let operations = OperationStore::open(database)?;
        Ok(Self {
            config: config.clone(),
            runtime: Arc::new(ControlRuntime::new(config)?),
            transport: Mutex::new(None),
            operations,
            policy,
            home_identity,
            runtime_boot_id: AtomicU64::new(0),
            policy_version,
        })
    }

    pub async fn start(&self, config: &ControlConfig, epoch: RuntimeEpoch) -> Result<(), String> {
        let transport = Arc::new(ControlTransport::start(config, epoch).await?);
        *self.transport.lock().await = Some(transport);
        self.runtime_boot_id.store(epoch.0, Ordering::Release);
        self.runtime.mark_ready();
        Ok(())
    }

    pub fn status(&self) -> ControlStatus {
        self.runtime.status()
    }

    pub async fn snapshot(&self) -> Result<CodexControlSnapshot, String> {
        let transport = self.transport().await?;
        let (account, models, config, mcp_servers, plugins, skills, hooks, apps) = tokio::join!(
            self.read_transport(
                &transport,
                ControlAction::AccountRead,
                json!({"refreshToken": false})
            ),
            self.read_transport(&transport, ControlAction::ModelsList, json!({})),
            self.read_transport(
                &transport,
                ControlAction::ConfigRead,
                json!({"includeLayers": true})
            ),
            self.read_transport(&transport, ControlAction::McpServersList, json!({})),
            self.read_transport(&transport, ControlAction::PluginsList, json!({})),
            self.read_transport(&transport, ControlAction::SkillsList, json!({})),
            self.read_transport(&transport, ControlAction::HooksList, json!({})),
            self.read_transport(&transport, ControlAction::AppsList, json!({})),
        );
        let account =
            serde_json::to_value(account_summary(&account?)).map_err(|error| error.to_string())?;
        let pending_server_requests = transport.pending_server_requests().await?;
        Ok(CodexControlSnapshot {
            status: self.runtime.status(),
            account,
            models: sanitize_value(models?),
            config: sanitize_value(config?),
            mcp_servers: sanitize_value(mcp_servers?),
            plugins: sanitize_value(plugins?),
            skills: sanitize_value(skills?),
            hooks: sanitize_value(hooks?),
            apps: sanitize_value(apps?),
            pending_server_requests,
        })
    }

    pub async fn read(&self, action: ControlAction, params: Value) -> Result<Value, String> {
        if action.is_mutation() {
            return Err("mutation action sent to read path".to_string());
        }
        let transport = self.transport().await?;
        self.read_transport(&transport, action, params)
            .await
            .map(sanitize_value)
    }

    async fn read_transport(
        &self,
        transport: &ControlTransport,
        action: ControlAction,
        params: Value,
    ) -> Result<Value, String> {
        self.runtime
            .with_read(|| transport.request(action.method(), params))
            .await
    }

    pub async fn create_operation(
        &self,
        mutation: MutationAction,
        intent: &OperationIntent,
    ) -> Result<ControlOperation, String> {
        let action = ControlAction::from(mutation);
        if intent.method != mutation.method() {
            return Err("Codex mutation method does not match typed action".to_string());
        }
        self.policy.authorize(&action)?;
        validate_mutation_params(&action, &intent.redacted_request)?;
        let transport = self.transport().await?;
        let mut trusted_intent = intent.clone();
        trusted_intent.target_home_identity = self.home_identity.clone();
        trusted_intent.runtime_boot_id = self.runtime_boot_id.load(Ordering::Acquire);
        trusted_intent.policy_version = self.policy_version.clone();
        trusted_intent.expected_revision = Some(canonical_revision(&action, &transport).await?);
        self.operations.create(&trusted_intent)
    }

    pub fn approve_operation(&self, id: i64, approver: &str) -> Result<String, String> {
        self.operations.approve(id, approver)
    }

    pub fn cancel_operation(&self, id: i64) -> Result<(), String> {
        self.operations.cancel(id)
    }

    pub fn unfinished_operations(&self) -> Result<Vec<ControlOperation>, String> {
        self.operations.unfinished(100)
    }

    pub async fn resolve_recovery(&self, id: i64) -> Result<(), String> {
        let recovery = self
            .operations
            .get_for_recovery(id)?
            .ok_or_else(|| "Codex operation is not awaiting recovery".to_string())?;
        let operation = recovery.operation;
        if !matches!(
            operation.phase,
            axon_codex::operations::OperationPhase::Executing
                | axon_codex::operations::OperationPhase::Ambiguous
                | axon_codex::operations::OperationPhase::RecoveryRequired
                | axon_codex::operations::OperationPhase::RollbackRequired
        ) {
            return Err("Codex operation is not awaiting recovery".to_string());
        }
        let current_home = home_identity(&self.config.control_home)?;
        if recovery.target_home_identity != current_home
            || recovery.policy_version != self.policy_version
        {
            self.operations.retain_recovery(
                id,
                "control target or write policy changed; automatic recovery is unsafe",
            )?;
            return Err(
                "Codex recovery remains unresolved: control home or write policy changed"
                    .to_string(),
            );
        }
        let action = action_from_method(&operation.method)?;
        if completion_strategy(&action) == CompletionStrategy::ResponseAcknowledged {
            if let Some(evidence) = operation.response_evidence.as_deref() {
                return self.operations.resolve_recovery(id, evidence);
            }
            self.operations.retain_recovery(
                id,
                "one-shot response was not durably observed; explicit operator disposition required without replay",
            )?;
            return Err(
                "Codex recovery requires explicit non-replay operator disposition".to_string(),
            );
        }
        let transport = self.transport().await?;
        let state = canonical_state(&action, &transport).await?;
        match verify_intended_effect(
            &action,
            &operation.redacted_request,
            None,
            &state.value,
            operation.expected_revision.as_deref(),
            Some(&state.revision),
        ) {
            EffectProof::Applied => self.operations.resolve_recovery(id, &state.revision),
            EffectProof::Absent(reason) | EffectProof::Unknown(reason) => {
                self.operations.retain_recovery(id, &reason)?;
                Err(format!("Codex recovery remains unresolved: {reason}"))
            }
        }
    }

    pub fn resolve_recovery_without_replay(
        &self,
        id: i64,
        applied: bool,
        note: &str,
    ) -> Result<(), String> {
        if note.trim().is_empty() {
            return Err("operator disposition note is required".to_string());
        }
        self.operations.resolve_without_replay(id, applied, note)
    }

    pub async fn execute_operation(
        &self,
        id: i64,
        capability: &str,
        mutation: MutationAction,
        params: Value,
    ) -> Result<Value, String> {
        let action = ControlAction::from(mutation);
        self.policy.authorize(&action)?;
        validate_mutation_params(&action, &params)?;
        let transport = self.transport().await?;
        let started = Arc::new(AtomicBool::new(false));
        let started_in_lane = Arc::clone(&started);
        let current_home = home_identity(&self.config.control_home)?;
        if current_home != self.home_identity {
            return Err("Codex control home identity changed; restart and reapprove".to_string());
        }
        let result = self
            .runtime
            .with_mutation(|| async {
                let before = canonical_state(&action, &transport).await?;
                self.operations.begin_execution(
                    id,
                    capability,
                    action.method(),
                    &params,
                    Some(&before.revision),
                    &self.home_identity,
                    self.runtime_boot_id.load(Ordering::Acquire),
                    &self.policy_version,
                )?;
                started_in_lane.store(true, Ordering::Release);
                let value = transport.request(action.method(), params.clone()).await?;
                let response_evidence = self
                    .operations
                    .record_response_evidence(id, &sanitize_value(value.clone()))?;
                if completion_strategy(&action) == CompletionStrategy::ResponseAcknowledged {
                    return Ok::<_, String>((value, response_evidence));
                }
                let after = canonical_state(&action, &transport)
                    .await
                    .map_err(|error| format!("post-state readback failed: {error}"))?;
                match verify_intended_effect(
                    &action,
                    &params,
                    Some(&before.value),
                    &after.value,
                    Some(&before.revision),
                    Some(&after.revision),
                ) {
                    EffectProof::Applied => Ok::<_, String>((value, after.revision)),
                    EffectProof::Absent(reason) | EffectProof::Unknown(reason) => Err(format!(
                        "post-state did not prove intended effect: {reason}"
                    )),
                }
            })
            .await;
        match result {
            Ok((value, post_revision)) => {
                self.operations.reconcile(id, &post_revision)?;
                Ok(sanitize_value(value))
            }
            Err(error) => {
                if started.load(Ordering::Acquire) {
                    self.operations.fail_ambiguous(id, &error)?;
                }
                Err(error)
            }
        }
    }

    pub async fn events_after(
        &self,
        cursor: Option<EventCursor>,
        limit: usize,
    ) -> Result<Vec<RecordedEvent>, String> {
        self.transport().await?.events_after(cursor, limit)
    }

    pub async fn respond_to_server_request(
        &self,
        boot_id: u64,
        request_id: u64,
        approved: bool,
        typed_response: Option<Value>,
    ) -> Result<(), String> {
        self.transport()
            .await?
            .respond_to_server_request(boot_id, request_id, approved, typed_response)
            .await
    }

    async fn transport(&self) -> Result<Arc<ControlTransport>, String> {
        let mut slot = self.transport.lock().await;
        if let Some(transport) = slot.as_ref() {
            if transport.is_alive() {
                return Ok(Arc::clone(transport));
            }
            self.runtime
                .record_restart_failure("Codex app-server exited; restarting on demand");
            *slot = None;
        }
        self.runtime.begin_restart().await?;
        let boot_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("system clock before epoch: {error}"))?
            .as_millis() as u64;
        let transport = match ControlTransport::start(&self.config, RuntimeEpoch(boot_id)).await {
            Ok(transport) => Arc::new(transport),
            Err(error) => {
                self.runtime.record_restart_failure(error.clone());
                return Err(error);
            }
        };
        self.runtime_boot_id.store(boot_id, Ordering::Release);
        self.runtime.mark_ready();
        *slot = Some(Arc::clone(&transport));
        Ok(transport)
    }
}

async fn canonical_revision(
    action: &ControlAction,
    transport: &ControlTransport,
) -> Result<String, String> {
    Ok(canonical_state(action, transport).await?.revision)
}

struct CanonicalState {
    value: Value,
    revision: String,
}

async fn canonical_state(
    action: &ControlAction,
    transport: &ControlTransport,
) -> Result<CanonicalState, String> {
    let (method, params) = match action {
        ControlAction::AccountLoginStart
        | ControlAction::AccountLoginCancel
        | ControlAction::AccountLogout => (
            ControlAction::AccountRead.method(),
            json!({"refreshToken":false}),
        ),
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
enum CompletionStrategy {
    CanonicalReadback,
    ResponseAcknowledged,
}

fn completion_strategy(action: &ControlAction) -> CompletionStrategy {
    if matches!(
        action,
        ControlAction::AccountLoginStart
            | ControlAction::AccountLoginCancel
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

fn action_from_method(method: &str) -> Result<ControlAction, String> {
    MutationAction::try_from(method).map(ControlAction::from)
}

#[cfg(test)]
#[path = "codex_control_tests.rs"]
mod tests;
