//! `action=watch` — the source-request-backed watch surface (issue #298
//! WS-B), mirroring the REST `/v1/watches` routes
//! (`docs/pipeline-unification/surfaces/rest-contract.md` Watch Routes).
//!
//! All MCP watch subactions route through the canonical source-watch store.

use super::AxonMcpServer;
use super::artifacts::{InlineHint, respond_with_mode};
use super::common::{invalid_params, logged_internal_error};
use super::system_requests::WatchMcpRequest;
use crate::schema::{AxonToolResponse, WatchSubaction};
use axon_api::source::{
    AdapterOptions, WatchExecRequest, WatchHistoryRequest, WatchId, WatchListRequest,
    WatchSchedule, WatchUpdateRequest,
};
use axon_services::watch::{self as watch_svc, SourceWatchStoreTrait};
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(super) async fn handle_watch(
        &self,
        req: WatchMcpRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let subaction = req.subaction.unwrap_or(WatchSubaction::List);
        match subaction {
            WatchSubaction::Create => self.watch_create(req).await,
            WatchSubaction::List => self.watch_list(req).await,
            WatchSubaction::Get => self.watch_get(req).await,
            WatchSubaction::Status => self.watch_status(req).await,
            WatchSubaction::Update => self.watch_update(req).await,
            WatchSubaction::Pause => self.watch_set_enabled(req, false).await,
            WatchSubaction::Resume => self.watch_set_enabled(req, true).await,
            WatchSubaction::Delete => self.watch_delete(req).await,
            WatchSubaction::Exec => self.watch_exec(req).await,
            WatchSubaction::History => self.watch_history(req).await,
        }
    }

    async fn open_store(&self) -> Result<watch_svc::SqliteWatchStore, ErrorData> {
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("watch.context", e.as_ref()))?;
        let pool = ctx.jobs.sqlite_pool();
        watch_svc::open_source_watch_store(self.cfg.as_ref(), pool.as_deref())
            .await
            .map_err(|e| logged_internal_error("watch.open_store", e.as_ref()))
    }

    async fn watch_create(&self, req: WatchMcpRequest) -> Result<AxonToolResponse, ErrorData> {
        let source = req
            .source
            .clone()
            .ok_or_else(|| invalid_params("watch subaction 'create' requires 'source'"))?;
        let every_seconds = req
            .every_seconds
            .ok_or_else(|| invalid_params("watch subaction 'create' requires 'every_seconds'"))?;
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("watch.context", e.as_ref()))?;
        let pool = ctx.jobs.sqlite_pool();
        let request = axon_api::source::WatchRequest {
            source,
            schedule: WatchSchedule {
                every_seconds: every_seconds.max(0) as u64,
                cron: None,
                timezone: None,
            },
            embed: req.embed.unwrap_or(true),
            options: AdapterOptions::default(),
            scope: None,
            collection: req.collection.clone(),
            enabled: req.enabled,
        };
        let created =
            watch_svc::create_source_watch(self.cfg.as_ref(), pool.as_deref(), request, None)
                .await
                .map_err(|e| invalid_params(e.to_string()))?;
        respond_with_mode(
            "watch",
            "create",
            req.response_mode,
            "watch-create",
            serde_json::json!(created),
            InlineHint::Default,
        )
        .await
    }

    async fn watch_list(&self, req: WatchMcpRequest) -> Result<AxonToolResponse, ErrorData> {
        let store = self.open_store().await?;
        let list_request = watch_list_request(&req);
        let page = SourceWatchStoreTrait::list(&store, list_request)
            .await
            .map_err(|e| invalid_params(e.to_string()))?;
        respond_with_mode(
            "watch",
            "list",
            req.response_mode,
            "watch-list",
            serde_json::json!(page),
            InlineHint::Default,
        )
        .await
    }

    fn require_id(req: &WatchMcpRequest) -> Result<WatchId, ErrorData> {
        req.id
            .clone()
            .map(WatchId::new)
            .ok_or_else(|| invalid_params("watch subaction requires 'id'"))
    }

    async fn resolve_watch_id(&self, req: &WatchMcpRequest) -> Result<WatchId, ErrorData> {
        let raw = req
            .id
            .as_deref()
            .or(req.source.as_deref())
            .ok_or_else(|| invalid_params("watch subaction requires 'id' or 'source'"))?;
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("watch.context", e.as_ref()))?;
        let pool = ctx.jobs.sqlite_pool();
        watch_svc::resolve_source_watch_id(self.cfg.as_ref(), pool.as_deref(), raw)
            .await
            .map_err(|e| invalid_params(e.to_string()))
    }

    async fn watch_get(&self, req: WatchMcpRequest) -> Result<AxonToolResponse, ErrorData> {
        let watch_id = Self::require_id(&req)?;
        let store = self.open_store().await?;
        let found = SourceWatchStoreTrait::get(&store, watch_id.clone())
            .await
            .map_err(|e| invalid_params(e.to_string()))?
            .ok_or_else(|| invalid_params(format!("watch {} not found", watch_id.0)))?;
        respond_with_mode(
            "watch",
            "get",
            req.response_mode,
            "watch-get",
            serde_json::json!(found),
            InlineHint::Default,
        )
        .await
    }

    async fn watch_status(&self, req: WatchMcpRequest) -> Result<AxonToolResponse, ErrorData> {
        let watch_id = self.resolve_watch_id(&req).await?;
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("watch.context", e.as_ref()))?;
        let store = self.open_store().await?;
        let found = SourceWatchStoreTrait::get(&store, watch_id.clone())
            .await
            .map_err(|e| invalid_params(e.to_string()))?
            .ok_or_else(|| invalid_params(format!("watch {} not found", watch_id.0)))?;
        let latest_job_summary = match found.latest_job.as_ref() {
            Some(job) => axon_services::jobs::unified_job_status(&ctx, job.job_id)
                .await
                .map_err(|e| logged_internal_error("watch.status", e.as_ref()))?,
            None => None,
        };
        respond_with_mode(
            "watch",
            "status",
            req.response_mode,
            "watch-status",
            serde_json::json!({
                "watch": found,
                "latest_job_summary": latest_job_summary,
            }),
            InlineHint::Default,
        )
        .await
    }

    async fn watch_update(&self, req: WatchMcpRequest) -> Result<AxonToolResponse, ErrorData> {
        let watch_id = Self::require_id(&req)?;
        let store = self.open_store().await?;
        let update = WatchUpdateRequest {
            enabled: req.enabled,
            schedule: req.every_seconds.map(|every_seconds| WatchSchedule {
                every_seconds: every_seconds.max(0) as u64,
                cron: None,
                timezone: None,
            }),
            options: None,
            embed: None,
            collection: req.collection.clone(),
            scope: None,
        };
        let updated = SourceWatchStoreTrait::update(&store, watch_id, update)
            .await
            .map_err(|e| invalid_params(e.to_string()))?;
        respond_with_mode(
            "watch",
            "update",
            req.response_mode,
            "watch-update",
            serde_json::json!(updated),
            InlineHint::Default,
        )
        .await
    }

    async fn watch_set_enabled(
        &self,
        req: WatchMcpRequest,
        enabled: bool,
    ) -> Result<AxonToolResponse, ErrorData> {
        let watch_id = Self::require_id(&req)?;
        let store = self.open_store().await?;
        let update = WatchUpdateRequest {
            enabled: Some(enabled),
            schedule: None,
            options: None,
            embed: None,
            collection: None,
            scope: None,
        };
        let subaction = if enabled { "resume" } else { "pause" };
        let updated = SourceWatchStoreTrait::update(&store, watch_id, update)
            .await
            .map_err(|e| invalid_params(e.to_string()))?;
        respond_with_mode(
            "watch",
            subaction,
            req.response_mode,
            "watch-update",
            serde_json::json!(updated),
            InlineHint::Default,
        )
        .await
    }

    async fn watch_delete(&self, req: WatchMcpRequest) -> Result<AxonToolResponse, ErrorData> {
        let watch_id = Self::require_id(&req)?;
        let store = self.open_store().await?;
        let deleted = store
            .delete(watch_id.clone())
            .await
            .map_err(|e| invalid_params(e.to_string()))?;
        if !deleted {
            return Err(invalid_params(format!("watch {} not found", watch_id.0)));
        }
        respond_with_mode(
            "watch",
            "delete",
            req.response_mode,
            "watch-delete",
            serde_json::json!({ "watch_id": watch_id.0, "deleted": true }),
            InlineHint::Default,
        )
        .await
    }

    async fn watch_exec(&self, req: WatchMcpRequest) -> Result<AxonToolResponse, ErrorData> {
        let watch_id = self.resolve_watch_id(&req).await?;
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("watch.context", e.as_ref()))?;
        let pool = ctx.jobs.sqlite_pool();
        let descriptor = watch_svc::exec_source_watch(
            &ctx,
            pool.as_deref(),
            watch_id,
            WatchExecRequest {
                reason: None,
                refresh: None,
                wait: None,
            },
            None,
        )
        .await
        .map_err(|e| logged_internal_error("watch.exec", e.as_ref()))?;
        respond_with_mode(
            "watch",
            "exec",
            req.response_mode,
            "watch-exec",
            serde_json::json!(descriptor),
            InlineHint::Default,
        )
        .await
    }

    async fn watch_history(&self, req: WatchMcpRequest) -> Result<AxonToolResponse, ErrorData> {
        let watch_id = self.resolve_watch_id(&req).await?;
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("watch.context", e.as_ref()))?;
        let pool = ctx.jobs.sqlite_pool();
        let history = watch_svc::history_source_watch(
            self.cfg.as_ref(),
            pool.as_deref(),
            watch_history_request(&req, watch_id),
        )
        .await
        .map_err(|e| invalid_params(e.to_string()))?;
        respond_with_mode(
            "watch",
            "history",
            req.response_mode,
            "watch-history",
            serde_json::json!(history),
            InlineHint::Default,
        )
        .await
    }
}

fn watch_list_request(req: &WatchMcpRequest) -> WatchListRequest {
    WatchListRequest {
        enabled: req.enabled,
        source_id: None,
        adapter: None,
        limit: req.limit.map(|limit| limit.max(0) as u32),
        cursor: req.cursor.clone(),
    }
}

fn watch_history_request(req: &WatchMcpRequest, watch_id: WatchId) -> WatchHistoryRequest {
    WatchHistoryRequest {
        watch_id,
        limit: req.limit.map(|limit| limit.max(0) as u32),
        cursor: req.cursor.clone(),
        status: req.status,
    }
}

#[cfg(test)]
#[path = "handlers_watch_tests.rs"]
mod tests;
