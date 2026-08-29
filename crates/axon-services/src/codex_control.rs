//! Services-first facade for the dedicated Codex app-server control plane.

use axon_codex::api::{
    ControlAction, WritePolicy, account_summary, state_revision, validate_mutation_params,
};
use axon_codex::control::{ControlConfig, ControlRuntime, ControlStatus, home_identity};
use axon_codex::events::sanitize_value;
use axon_codex::events::{EventCursor, RecordedEvent};
use axon_codex::operations::{ControlOperation, OperationIntent, OperationStore};
use axon_codex::protocol::RuntimeEpoch;
use axon_codex::transport::ControlTransport;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::Mutex;
use utoipa::ToSchema;

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
        operations.recover_interrupted()?;
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
        self.runtime.with_read(|| async {
            let (account, models, config, mcp_servers, plugins, skills, hooks, apps) = tokio::join!(
                transport.request(ControlAction::AccountRead.method(), json!({"refreshToken": false})),
                transport.request(ControlAction::ModelsList.method(), json!({})),
                transport.request(ControlAction::ConfigRead.method(), json!({"includeLayers": true})),
                transport.request(ControlAction::McpServersList.method(), json!({})),
                transport.request(ControlAction::PluginsList.method(), json!({})),
                transport.request(ControlAction::SkillsList.method(), json!({})),
                transport.request(ControlAction::HooksList.method(), json!({})),
                transport.request(ControlAction::AppsList.method(), json!({})),
            );
            let account = serde_json::to_value(account_summary(&account?)).map_err(|error| error.to_string())?;
            Ok(CodexControlSnapshot { status: self.runtime.status(), account, models: sanitize_value(models?), config: sanitize_value(config?), mcp_servers: sanitize_value(mcp_servers?), plugins: sanitize_value(plugins?), skills: sanitize_value(skills?), hooks: sanitize_value(hooks?), apps: sanitize_value(apps?) })
        }).await
    }

    pub async fn read(&self, action: ControlAction, params: Value) -> Result<Value, String> {
        if action.is_mutation() {
            return Err("mutation action sent to read path".to_string());
        }
        let transport = self.transport().await?;
        self.runtime
            .with_read(|| transport.request(action.method(), params))
            .await
            .map(sanitize_value)
    }

    pub async fn create_operation(
        &self,
        intent: &OperationIntent,
    ) -> Result<ControlOperation, String> {
        let action = action_from_method(&intent.method)?;
        self.policy.authorize(&action)?;
        validate_mutation_params(&action, &intent.redacted_request)?;
        verify_artifact_source(&action, &intent.redacted_request).await?;
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

    pub fn unfinished_operations(&self) -> Result<Vec<ControlOperation>, String> {
        self.operations.unfinished(100)
    }

    pub async fn resolve_recovery(&self, id: i64) -> Result<(), String> {
        let operation = self
            .operations
            .unfinished(100)?
            .into_iter()
            .find(|operation| operation.id == id)
            .ok_or_else(|| "Codex operation is not awaiting recovery".to_string())?;
        let action = action_from_method(&operation.method)?;
        let transport = self.transport().await?;
        let revision = canonical_revision(&action, &transport).await?;
        self.operations.resolve_recovery(id, &revision)
    }

    pub async fn execute_operation(
        &self,
        id: i64,
        capability: &str,
        action: ControlAction,
        params: Value,
    ) -> Result<Value, String> {
        self.policy.authorize(&action)?;
        validate_mutation_params(&action, &params)?;
        verify_artifact_source(&action, &params).await?;
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
                let revision = canonical_revision(&action, &transport).await?;
                self.operations.begin_execution(
                    id,
                    capability,
                    action.method(),
                    &params,
                    Some(&revision),
                    &self.home_identity,
                    self.runtime_boot_id.load(Ordering::Acquire),
                    &self.policy_version,
                )?;
                started_in_lane.store(true, Ordering::Release);
                let value = transport.request(action.method(), params).await?;
                let post_revision = canonical_revision(&action, &transport)
                    .await
                    .map_err(|error| format!("post-state readback failed: {error}"))?;
                Ok::<_, String>((value, post_revision))
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
    ) -> Result<(), String> {
        self.transport()
            .await?
            .respond_to_server_request(boot_id, request_id, approved)
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
        ControlAction::PluginInstall
        | ControlAction::PluginUninstall
        | ControlAction::MarketplaceAdd
        | ControlAction::MarketplaceRemove
        | ControlAction::MarketplaceUpgrade => (ControlAction::PluginsList.method(), json!({})),
        ControlAction::SkillConfigWrite | ControlAction::ExternalAgentConfigImport => {
            (ControlAction::SkillsList.method(), json!({}))
        }
        _ => (
            ControlAction::ConfigRead.method(),
            json!({"includeLayers":true}),
        ),
    };
    state_revision(&sanitize_value(transport.request(method, params).await?))
}

async fn verify_artifact_source(action: &ControlAction, params: &Value) -> Result<(), String> {
    if !matches!(
        action,
        ControlAction::PluginInstall
            | ControlAction::MarketplaceAdd
            | ControlAction::ExternalAgentConfigImport
    ) {
        return Ok(());
    }
    let source = params
        .get("source")
        .and_then(Value::as_str)
        .ok_or("artifact source missing")?;
    let expected = params
        .get("sha256")
        .and_then(Value::as_str)
        .ok_or("artifact digest missing")?;
    axon_core::http::validate_url_with_dns(source)
        .await
        .map_err(|error| format!("artifact source rejected: {error}"))?;
    let response = axon_core::http::http_client()
        .map_err(|error| error.to_string())?
        .get(source)
        .send()
        .await
        .map_err(|error| format!("artifact fetch failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("artifact fetch failed: {error}"))?;
    let mut stream = response.bytes_stream();
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("artifact fetch failed: {error}"))?;
        size = size.saturating_add(chunk.len() as u64);
        if size > axon_codex::artifacts::MAX_EXPANDED_BYTES {
            return Err("artifact download exceeds size limit".to_string());
        }
        hasher.update(&chunk);
    }
    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected) {
        return Err("artifact SHA-256 does not match approved digest".to_string());
    }
    Ok(())
}

fn action_from_method(method: &str) -> Result<ControlAction, String> {
    [
        ControlAction::AccountLoginStart,
        ControlAction::AccountLoginCancel,
        ControlAction::AccountLogout,
        ControlAction::ConfigValueWrite,
        ControlAction::ConfigBatchWrite,
        ControlAction::McpServerReload,
        ControlAction::McpServerOauthLogin,
        ControlAction::PluginInstall,
        ControlAction::PluginUninstall,
        ControlAction::MarketplaceAdd,
        ControlAction::MarketplaceRemove,
        ControlAction::MarketplaceUpgrade,
        ControlAction::SkillConfigWrite,
        ControlAction::ExternalAgentConfigImport,
    ]
    .into_iter()
    .find(|action| action.method() == method)
    .ok_or_else(|| format!("unsupported Codex control mutation: {method}"))
}
