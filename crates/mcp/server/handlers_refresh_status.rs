use super::AxonMcpServer;
use super::common::{
    invalid_params, logged_internal_error, parse_job_id, parse_limit, parse_offset,
    respond_with_mode, validate_mcp_urls,
};
use crate::crates::jobs::backend::{JobKind, JobPayload};
use crate::crates::mcp::schema::{
    AxonToolResponse, RefreshRequest, RefreshSubaction, ResponseMode, StatusRequest,
};
use crate::crates::services::jobs as job_service;
use crate::crates::services::refresh::{self as refresh_service, RefreshScheduleCreate};
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(super) async fn handle_status(
        &self,
        req: StatusRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("status.context", e.as_ref()))?;
        let result = crate::crates::services::system::full_status(service_context.as_ref())
            .await
            .map_err(|e| logged_internal_error("status", e.as_ref()))?;

        respond_with_mode(
            "status",
            "status",
            response_mode,
            "status",
            serde_json::json!({
                "text": result.text,
                "json": result.payload,
            }),
        )
        .await
    }

    pub(super) async fn handle_refresh(
        &self,
        req: RefreshRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        match req.subaction.unwrap_or(RefreshSubaction::Start) {
            RefreshSubaction::Start => self.handle_refresh_start(req.urls, req.url).await,
            RefreshSubaction::Status => {
                self.handle_refresh_status(req.job_id.as_deref(), response_mode)
                    .await
            }
            RefreshSubaction::Cancel => self.handle_refresh_cancel(req.job_id.as_deref()).await,
            RefreshSubaction::List => {
                self.handle_refresh_list(req.limit, req.offset, response_mode)
                    .await
            }
            RefreshSubaction::Cleanup => self.handle_refresh_cleanup().await,
            RefreshSubaction::Clear => self.handle_refresh_clear().await,
            RefreshSubaction::Recover => self.handle_refresh_recover().await,
            RefreshSubaction::Schedule => {
                self.handle_refresh_schedule(
                    req.schedule_subaction,
                    req.schedule_name,
                    req.urls,
                    req.url,
                    req.limit,
                    response_mode,
                )
                .await
            }
        }
    }

    async fn handle_refresh_start(
        &self,
        urls: Option<Vec<String>>,
        url: Option<String>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let urls = urls
            .or_else(|| url.map(|u| vec![u]))
            .ok_or_else(|| invalid_params("urls or url is required for refresh.start"))?;
        if urls.is_empty() {
            return Err(invalid_params("urls cannot be empty"));
        }
        validate_mcp_urls(&urls)?;
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("refresh.start.context", e.as_ref()))?;
        // Enqueue via ServiceContext so both full (Postgres) and lite (SQLite) backends work.
        // The backend payload takes one URL per job; enqueue one per URL and collect all ids.
        // If any enqueue fails, we include the already-created IDs in the error so the caller
        // can cancel orphaned jobs.
        let mut job_ids: Vec<uuid::Uuid> = Vec::with_capacity(urls.len());
        for url in &urls {
            match service_context
                .jobs
                .enqueue(JobPayload::Refresh {
                    url: url.clone(),
                    config_json: "{}".into(),
                })
                .await
            {
                Ok(id) => job_ids.push(id),
                Err(e) => {
                    let msg = format!(
                        "refresh.start failed on url {url} after enqueuing {} jobs",
                        job_ids.len()
                    );
                    tracing::error!("{msg}: {e}");
                    return Err(ErrorData::internal_error(
                        "refresh.start.partial failed".to_string(),
                        Some(serde_json::json!({
                            "partial_job_ids": job_ids,
                        })),
                    ));
                }
            }
        }
        Ok(AxonToolResponse::ok(
            "refresh",
            "start",
            serde_json::json!({
                "job_ids": job_ids,
                "job_id": job_ids.last().copied().unwrap_or(uuid::Uuid::nil()),
            }),
        ))
    }

    async fn handle_refresh_status(
        &self,
        job_id: Option<&str>,
        response_mode: Option<ResponseMode>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let id = parse_job_id(job_id)?;
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("refresh.status.context", e.as_ref()))?;
        let job = job_service::job_status(&service_context, JobKind::Refresh, id)
            .await
            .map_err(|e| logged_internal_error("refresh.status", e.as_ref()))?;
        let job_val = match job {
            Some(j) => serde_json::to_value(&j)
                .map_err(|e| logged_internal_error("refresh.status.serialize", &e))?,
            None => serde_json::Value::Null,
        };
        respond_with_mode(
            "refresh",
            "status",
            response_mode,
            &format!("refresh-status-{id}"),
            serde_json::json!({ "job": job_val }),
        )
        .await
    }

    async fn handle_refresh_cancel(
        &self,
        job_id: Option<&str>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let id = parse_job_id(job_id)?;
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("refresh.cancel.context", e.as_ref()))?;
        let canceled = job_service::cancel_job(&service_context, JobKind::Refresh, id)
            .await
            .map_err(|e| logged_internal_error("refresh.cancel", e.as_ref()))?;
        Ok(AxonToolResponse::ok(
            "refresh",
            "cancel",
            serde_json::json!({ "job_id": id.to_string(), "canceled": canceled }),
        ))
    }

    async fn handle_refresh_list(
        &self,
        limit: Option<i64>,
        offset: Option<usize>,
        response_mode: Option<ResponseMode>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let limit = parse_limit(limit, 20);
        let offset = parse_offset(offset);
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("refresh.list.context", e.as_ref()))?;
        let jobs = job_service::list_jobs(
            &service_context,
            JobKind::Refresh,
            limit,
            i64::try_from(offset).unwrap_or(i64::MAX),
        )
        .await
        .map_err(|e| logged_internal_error("refresh.list", e.as_ref()))?;
        let jobs_val: Result<Vec<_>, _> = jobs.iter().map(serde_json::to_value).collect();
        let jobs_val = jobs_val.map_err(|e| logged_internal_error("refresh.list.serialize", &e))?;
        respond_with_mode(
            "refresh",
            "list",
            response_mode,
            "refresh-list",
            serde_json::json!({
                "jobs": jobs_val,
                "limit": limit,
                "offset": offset,
            }),
        )
        .await
    }

    async fn handle_refresh_cleanup(&self) -> Result<AxonToolResponse, ErrorData> {
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("refresh.cleanup.context", e.as_ref()))?;
        let deleted = job_service::cleanup_jobs(&service_context, JobKind::Refresh)
            .await
            .map_err(|e| logged_internal_error("refresh.cleanup", e.as_ref()))?;
        Ok(AxonToolResponse::ok(
            "refresh",
            "cleanup",
            serde_json::json!({ "deleted": deleted }),
        ))
    }

    async fn handle_refresh_clear(&self) -> Result<AxonToolResponse, ErrorData> {
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("refresh.clear.context", e.as_ref()))?;
        let deleted = job_service::clear_jobs(&service_context, JobKind::Refresh)
            .await
            .map_err(|e| logged_internal_error("refresh.clear", e.as_ref()))?;
        Ok(AxonToolResponse::ok(
            "refresh",
            "clear",
            serde_json::json!({ "deleted": deleted }),
        ))
    }

    async fn handle_refresh_recover(&self) -> Result<AxonToolResponse, ErrorData> {
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("refresh.recover.context", e.as_ref()))?;
        let recovered = job_service::recover_jobs(&service_context, JobKind::Refresh)
            .await
            .map_err(|e| logged_internal_error("refresh.recover", e.as_ref()))?;
        Ok(AxonToolResponse::ok(
            "refresh",
            "recover",
            serde_json::json!({ "recovered": recovered }),
        ))
    }

    async fn handle_refresh_schedule(
        &self,
        schedule_subaction: Option<String>,
        schedule_name: Option<String>,
        urls: Option<Vec<String>>,
        url: Option<String>,
        limit: Option<i64>,
        response_mode: Option<ResponseMode>,
    ) -> Result<AxonToolResponse, ErrorData> {
        // Validate subaction early — before connecting to Postgres — so unknown
        // subaction errors are always INVALID_PARAMS regardless of DB availability.
        let sub = schedule_subaction.as_deref().unwrap_or("list");
        match sub {
            "list" | "create" | "delete" | "enable" | "disable" => {}
            other => {
                return Err(invalid_params(format!(
                    "unknown schedule_subaction: {other}; expected list, create, delete, enable, disable"
                )));
            }
        }
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("refresh.schedule.context", e.as_ref()))?;
        if !service_context.capabilities.refresh_schedule.supported {
            return Err(invalid_params(
                service_context
                    .capabilities
                    .refresh_schedule
                    .reason
                    .unwrap_or("refresh scheduling is not available in this mode"),
            ));
        }
        match sub {
            "list" => {
                self.handle_refresh_schedule_list(limit, response_mode)
                    .await
            }
            "create" => {
                self.handle_refresh_schedule_create(schedule_name, urls, url)
                    .await
            }
            "delete" => self.handle_refresh_schedule_delete(schedule_name).await,
            "enable" => {
                self.handle_refresh_schedule_set_enabled(schedule_name, true)
                    .await
            }
            "disable" => {
                self.handle_refresh_schedule_set_enabled(schedule_name, false)
                    .await
            }
            // Unreachable: early validation above catches all other values.
            _ => unreachable!("schedule_subaction already validated"),
        }
    }

    async fn handle_refresh_schedule_list(
        &self,
        limit: Option<i64>,
        response_mode: Option<ResponseMode>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let limit = parse_limit(limit, 20);
        let schedules = refresh_service::refresh_schedule_list(self.cfg.as_ref(), limit)
            .await
            .map_err(|e| logged_internal_error("refresh.schedule.list", e.as_ref()))?;
        respond_with_mode(
            "refresh",
            "schedule",
            response_mode,
            "refresh-schedules",
            serde_json::json!({ "schedules": schedules }),
        )
        .await
    }

    async fn handle_refresh_schedule_create(
        &self,
        schedule_name: Option<String>,
        urls: Option<Vec<String>>,
        url: Option<String>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let name = schedule_name
            .ok_or_else(|| invalid_params("schedule_name is required for schedule create"))?;
        let urls = urls.or_else(|| url.map(|u| vec![u])).unwrap_or_default();
        if urls.is_empty() {
            return Err(invalid_params(
                "refresh schedule create requires at least one URL",
            ));
        }
        validate_mcp_urls(&urls)?;
        let schedule = refresh_service::refresh_schedule_create(
            self.cfg.as_ref(),
            &RefreshScheduleCreate {
                name,
                seed_url: None,
                urls: Some(urls),
                every_seconds: 3600,
                enabled: true,
                next_run_at: chrono::Utc::now(),
                source_type: None,
                target: None,
            },
        )
        .await
        .map_err(|e| logged_internal_error("refresh.schedule.create", e.as_ref()))?;
        Ok(AxonToolResponse::ok(
            "refresh",
            "schedule",
            serde_json::json!({ "created": schedule }),
        ))
    }

    async fn handle_refresh_schedule_delete(
        &self,
        schedule_name: Option<String>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let name = schedule_name
            .ok_or_else(|| invalid_params("schedule_name is required for schedule delete"))?;
        let deleted = refresh_service::refresh_schedule_delete(self.cfg.as_ref(), &name)
            .await
            .map_err(|e| logged_internal_error("refresh.schedule.delete", e.as_ref()))?;
        Ok(AxonToolResponse::ok(
            "refresh",
            "schedule",
            serde_json::json!({ "name": name, "deleted": deleted }),
        ))
    }

    async fn handle_refresh_schedule_set_enabled(
        &self,
        schedule_name: Option<String>,
        enabled: bool,
    ) -> Result<AxonToolResponse, ErrorData> {
        let name = schedule_name.ok_or_else(|| {
            let action = if enabled { "enable" } else { "disable" };
            invalid_params(format!("schedule_name is required for schedule {action}"))
        })?;
        let updated = if enabled {
            refresh_service::refresh_schedule_enable(self.cfg.as_ref(), &name)
                .await
                .map_err(|e| logged_internal_error("refresh.schedule.enable", e.as_ref()))?
        } else {
            refresh_service::refresh_schedule_disable(self.cfg.as_ref(), &name)
                .await
                .map_err(|e| logged_internal_error("refresh.schedule.disable", e.as_ref()))?
        };
        Ok(AxonToolResponse::ok(
            "refresh",
            "schedule",
            serde_json::json!({ "name": name, "enabled": enabled, "updated": updated }),
        ))
    }
}
