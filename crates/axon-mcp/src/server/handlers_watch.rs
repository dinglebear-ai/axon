//! `action=watch` — the source-request-backed watch surface (issue #298
//! WS-B), mirroring the REST `/v1/watches` routes
//! (`docs/pipeline-unification/surfaces/rest-contract.md` Watch Routes).
//!
//! This is a distinct storage model from the legacy `WatchSubaction::Create`
//! `/`List`/`Get`/`Exec`/`History` task_type/task_payload watches — see
//! `crates/axon-jobs/src/watch_store.rs` module docs. Only `list`, `get`,
//! `update`, `pause`, `resume`, and `delete` are wired here; the legacy
//! subactions remain unimplemented over MCP (available through the HTTP API
//! per the existing `AxonRequest::Watch` rejection message).

use super::AxonMcpServer;
use super::artifacts::{InlineHint, respond_with_mode};
use super::common::{invalid_params, logged_internal_error};
use crate::schema::{AxonToolResponse, WatchRequest, WatchSubaction};
use axon_api::source::{WatchId, WatchListRequest, WatchUpdateRequest};
use axon_services::watch::{self as watch_svc, SourceWatchStoreTrait};
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(super) async fn handle_watch(
        &self,
        req: WatchRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let subaction = req.subaction.unwrap_or(WatchSubaction::List);
        match subaction {
            WatchSubaction::List => self.watch_list(req).await,
            WatchSubaction::Get => self.watch_get(req).await,
            WatchSubaction::Update => self.watch_update(req).await,
            WatchSubaction::Pause => self.watch_set_enabled(req, false).await,
            WatchSubaction::Resume => self.watch_set_enabled(req, true).await,
            WatchSubaction::Delete => self.watch_delete(req).await,
            WatchSubaction::Create | WatchSubaction::Exec | WatchSubaction::History => {
                Err(invalid_params(
                    "watch subaction 'create'/'exec'/'history' is available through the HTTP API, not MCP",
                ))
            }
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

    async fn watch_list(&self, req: WatchRequest) -> Result<AxonToolResponse, ErrorData> {
        let store = self.open_store().await?;
        let list_request = WatchListRequest {
            enabled: req.enabled,
            source_id: None,
            adapter: None,
            limit: req.limit.map(|limit| limit.max(0) as u32),
            cursor: None,
        };
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

    fn require_id(req: &WatchRequest) -> Result<WatchId, ErrorData> {
        req.id
            .clone()
            .map(WatchId::new)
            .ok_or_else(|| invalid_params("watch subaction requires 'id'"))
    }

    async fn watch_get(&self, req: WatchRequest) -> Result<AxonToolResponse, ErrorData> {
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

    async fn watch_update(&self, req: WatchRequest) -> Result<AxonToolResponse, ErrorData> {
        let watch_id = Self::require_id(&req)?;
        let store = self.open_store().await?;
        let update = WatchUpdateRequest {
            enabled: req.enabled,
            schedule: req
                .every_seconds
                .map(|every_seconds| axon_api::source::WatchSchedule {
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
        req: WatchRequest,
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

    async fn watch_delete(&self, req: WatchRequest) -> Result<AxonToolResponse, ErrorData> {
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
}
