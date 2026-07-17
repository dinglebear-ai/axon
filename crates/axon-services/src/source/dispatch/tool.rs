//! CLI/MCP tool dispatch through the canonical non-web source pipeline.

use async_trait::async_trait;
use axon_adapters::{
    SourceAdapter, acquisition::MaterializedSource, cli_tool::CliToolSourceAdapter,
    mcp_tool::McpToolSourceAdapter,
};
use axon_api::source::{
    AdapterRef, AuthSnapshot, ConfigSnapshotId, EffectiveLimits, LifecycleStatus, PipelinePhase,
    Severity, SourceAcquisition, SourceAdapterCapability, SourceKind, SourceLimits, SourceManifest,
    SourceManifestDiff, SourcePlan, SourceRequest, StageExecutionResult, Visibility,
};
use axon_core::logging::log_info;
use sha2::{Digest, Sha256};

use super::dispatch_materialized;
use super::tool_artifacts::capture_tool_output_artifacts;
use super::tool_auth::{
    AuthorizedToolExecution, ToolExecutionPolicy, authorize_cli_tool_execution,
    authorize_mcp_tool_execution,
};
use crate::context::TargetLocalSourceRuntime;
use crate::source::SourceExecutionContext;
use crate::source::result_map::IndexCounts;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_cli_tool(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    route: &axon_api::source::RoutePlan,
    execution: &SourceExecutionContext,
    policy: &ToolExecutionPolicy,
) -> anyhow::Result<IndexCounts> {
    let authorization = authorize_cli_tool_execution(input, auth_snapshot, route, policy)?;
    dispatch_tool_adapter(
        runtime,
        input,
        collection,
        owner_id,
        auth_snapshot,
        embed,
        route,
        execution,
        SourceKind::CliTool,
        "cli_tool",
        &CliToolSourceAdapter::new(),
        authorization,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_mcp_tool(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    route: &axon_api::source::RoutePlan,
    execution: &SourceExecutionContext,
    policy: &ToolExecutionPolicy,
) -> anyhow::Result<IndexCounts> {
    let authorization = authorize_mcp_tool_execution(input, auth_snapshot, route, policy)?;
    dispatch_tool_adapter(
        runtime,
        input,
        collection,
        owner_id,
        auth_snapshot,
        embed,
        route,
        execution,
        SourceKind::McpTool,
        "mcp_tool",
        &McpToolSourceAdapter::new(),
        authorization,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_tool_adapter(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    route: &axon_api::source::RoutePlan,
    execution: &SourceExecutionContext,
    source_kind: SourceKind,
    adapter_name: &'static str,
    adapter: &dyn SourceAdapter,
    authorization: AuthorizedToolExecution,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source kind={adapter_name} mode={} target_id={}",
        if authorization.execute {
            "execute"
        } else {
            "metadata"
        },
        redacted_target_id(input)
    ));
    let plan = tool_plan(route, source_kind, adapter_name, embed, &authorization);
    let audited = AuditedToolAdapter {
        inner: adapter,
        runtime,
        authorization,
        caller_id: auth_snapshot.and_then(|snapshot| snapshot.caller_id.clone()),
        raw_input: input,
    };
    dispatch_materialized(
        runtime,
        &audited,
        plan,
        collection,
        owner_id,
        auth_snapshot,
        execution,
        |plan| async { Ok(MaterializedSource::virtual_source(plan)) },
    )
    .await
}

struct AuditedToolAdapter<'a> {
    inner: &'a dyn SourceAdapter,
    runtime: &'a TargetLocalSourceRuntime,
    authorization: AuthorizedToolExecution,
    caller_id: Option<String>,
    raw_input: &'a str,
}

#[async_trait]
impl SourceAdapter for AuditedToolAdapter<'_> {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn version(&self) -> &'static str {
        self.inner.version()
    }

    async fn capabilities(&self) -> axon_adapters::adapter::Result<SourceAdapterCapability> {
        self.inner.capabilities().await
    }

    async fn discover(&self, plan: &SourcePlan) -> axon_adapters::adapter::Result<SourceManifest> {
        self.inner.discover(&self.plan_with_policy(plan)).await
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> axon_adapters::adapter::Result<SourceAcquisition> {
        if self.authorization.execute {
            persist_execution_audit(
                self.runtime,
                plan,
                self.authorization.policy_id,
                self.caller_id.as_deref(),
            )
            .await
            .map_err(|error| {
                let safe_error = axon_core::redact::redact_secrets(&error.to_string());
                tracing::error!(job_id = %plan.job_id.0, error = %safe_error, "tool audit persistence failed");
                axon_api::source::ApiError::new(
                    "tool.audit_persist_failed",
                    axon_error::ErrorStage::Authorizing,
                    "tool execution blocked because durable audit persistence failed",
                )
            })?;
        }
        let mut acquisition = self
            .inner
            .acquire(&self.plan_with_policy(plan), diff)
            .await?;
        capture_tool_output_artifacts(self.runtime, plan, &mut acquisition)
            .await
            .map_err(|error| {
                let safe_error = axon_core::redact::redact_secrets(&error.to_string());
                tracing::error!(job_id = %plan.job_id.0, error = %safe_error, "tool artifact persistence failed");
                axon_api::source::ApiError::new(
                    "tool.artifact_persist_failed",
                    axon_error::ErrorStage::Publishing,
                    "tool output artifact persistence failed",
                )
            })?;
        Ok(acquisition)
    }

    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> axon_adapters::adapter::Result<StageExecutionResult<Vec<axon_api::source::SourceDocument>>>
    {
        self.inner
            .normalize(&self.plan_with_policy(plan), acquisition)
            .await
    }
}

impl AuditedToolAdapter<'_> {
    fn plan_with_policy(&self, plan: &SourcePlan) -> SourcePlan {
        let mut plan = plan.clone();
        plan.request.source = self.raw_input.to_string();
        plan.request.metadata.insert(
            "tool_execution_policy".to_string(),
            self.authorization.policy_metadata.clone(),
        );
        plan
    }
}

fn tool_plan(
    routed: &axon_api::source::RoutePlan,
    source_kind: SourceKind,
    adapter_name: &'static str,
    embed: bool,
    authorization: &AuthorizedToolExecution,
) -> SourcePlan {
    let adapter = AdapterRef {
        name: adapter_name.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let mut route = routed.clone();
    route.adapter = adapter.clone();
    route.source.adapter = adapter;
    route.source.source_kind = source_kind;

    let mut request = SourceRequest::new(route.source.canonical_uri.clone());
    request.scope = Some(route.scope);
    request.adapter = Some(adapter_name.to_string());
    request.embed = embed;
    request.options = invocation_options(routed);
    request.metadata.insert(
        "tool_execute_authorized".to_string(),
        serde_json::json!(authorization.execute),
    );
    SourcePlan {
        job_id: super::placeholder_job_id(),
        request,
        route,
        stage_plan: Vec::new(),
        limits: EffectiveLimits {
            request: SourceLimits::default(),
            adapter_defaults: SourceLimits::default(),
            config_defaults: SourceLimits::default(),
            effective: SourceLimits::default(),
        },
        config_snapshot_id: ConfigSnapshotId::new("cfg_tool_source"),
        provider_reservations: Vec::new(),
    }
}

fn invocation_options(route: &axon_api::source::RoutePlan) -> axon_api::source::AdapterOptions {
    const CALLER_POLICY_KEYS: &[&str] = &[
        "command_allowlist",
        "tool_allowlist",
        "mcp_allowlist",
        "mcp_caller_command",
        "mcp_caller_allowlist",
        "env_allowlist",
        "side_effect_class",
        "timeout_ms",
        "output_cap_bytes",
    ];
    let mut options = route.validated_options.clone();
    for key in CALLER_POLICY_KEYS {
        options.values.0.remove(*key);
    }
    options
}

async fn persist_execution_audit(
    runtime: &TargetLocalSourceRuntime,
    plan: &SourcePlan,
    policy_id: &str,
    caller_id: Option<&str>,
) -> anyhow::Result<()> {
    let sequence = runtime
        .jobs
        .latest_event_sequence(plan.job_id)
        .await?
        .unwrap_or(0)
        + 1;
    let mut event = axon_api::source::SourceProgressEvent::minimal(
        plan.job_id,
        sequence,
        PipelinePhase::Authorizing,
        LifecycleStatus::Completed,
        Severity::Info,
        format!(
            "tool execution authorized policy={policy_id} target_id={} caller_id={}",
            redacted_target_id(&plan.request.source),
            caller_id.unwrap_or("trusted-local")
        ),
    );
    event.visibility = Visibility::Internal;
    event.source_id = Some(plan.route.source.source_id.clone());
    event.adapter = Some(plan.route.adapter.clone());
    event.scope = Some(plan.route.scope);
    event.dedupe_key = Some(format!("tool-audit:{}", plan.job_id.0));
    runtime.jobs.append_event(event).await?;
    Ok(())
}

fn redacted_target_id(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())[..16].to_string()
}
