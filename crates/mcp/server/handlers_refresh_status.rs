use super::AxonMcpServer;
use super::common::{
    invalid_params, logged_internal_error, parse_job_id, parse_limit, parse_offset,
    respond_with_mode,
};
use crate::crates::core::http::validate_url;
use crate::crates::mcp::schema::{
    AxonToolResponse, RefreshRequest, RefreshSubaction, ResponseMode, StatusRequest,
};
use crate::crates::services::refresh::{self as refresh_service, RefreshScheduleCreate};
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(super) async fn handle_status(
        &self,
        req: StatusRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        let result = crate::crates::services::system::full_status(self.cfg.as_ref())
            .await
            .map_err(|e| logged_internal_error("status", e))?;

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
                self.handle_refresh_status(req.job_id.as_ref(), response_mode)
                    .await
            }
            RefreshSubaction::Cancel => self.handle_refresh_cancel(req.job_id.as_ref()).await,
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
        let result = refresh_service::refresh_start(self.cfg.as_ref(), &urls)
            .await
            .map_err(|e| logged_internal_error("refresh.start", e))?;
        Ok(AxonToolResponse::ok(
            "refresh",
            "start",
            serde_json::json!({ "job_id": result.job_id }),
        ))
    }

    async fn handle_refresh_status(
        &self,
        job_id: Option<&String>,
        response_mode: Option<ResponseMode>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let id = parse_job_id(job_id)?;
        let job = refresh_service::refresh_status(self.cfg.as_ref(), id)
            .await
            .map_err(|e| logged_internal_error("refresh.status", e))?;
        respond_with_mode(
            "refresh",
            "status",
            response_mode,
            &format!("refresh-status-{id}"),
            serde_json::json!({ "job": job.map(|j| j.payload) }),
        )
        .await
    }

    async fn handle_refresh_cancel(
        &self,
        job_id: Option<&String>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let id = parse_job_id(job_id)?;
        let canceled = refresh_service::refresh_cancel(self.cfg.as_ref(), id)
            .await
            .map_err(|e| logged_internal_error("refresh.cancel", e))?;
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
        let jobs = refresh_service::refresh_list(self.cfg.as_ref(), limit, offset as i64)
            .await
            .map_err(|e| logged_internal_error("refresh.list", e))?;
        respond_with_mode(
            "refresh",
            "list",
            response_mode,
            "refresh-list",
            serde_json::json!({ "jobs": jobs.payload, "limit": limit, "offset": offset }),
        )
        .await
    }

    async fn handle_refresh_cleanup(&self) -> Result<AxonToolResponse, ErrorData> {
        let deleted = refresh_service::refresh_cleanup(self.cfg.as_ref())
            .await
            .map_err(|e| logged_internal_error("refresh.cleanup", e))?;
        Ok(AxonToolResponse::ok(
            "refresh",
            "cleanup",
            serde_json::json!({ "deleted": deleted }),
        ))
    }

    async fn handle_refresh_clear(&self) -> Result<AxonToolResponse, ErrorData> {
        let deleted = refresh_service::refresh_clear(self.cfg.as_ref())
            .await
            .map_err(|e| logged_internal_error("refresh.clear", e))?;
        Ok(AxonToolResponse::ok(
            "refresh",
            "clear",
            serde_json::json!({ "deleted": deleted }),
        ))
    }

    async fn handle_refresh_recover(&self) -> Result<AxonToolResponse, ErrorData> {
        let recovered = refresh_service::refresh_recover(self.cfg.as_ref())
            .await
            .map_err(|e| logged_internal_error("refresh.recover", e))?;
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
        let sub = schedule_subaction.as_deref().unwrap_or("list");
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
            other => Err(invalid_params(format!(
                "unknown schedule_subaction: {other}; expected list, create, delete, enable, disable"
            ))),
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
            .map_err(|e| logged_internal_error("refresh.schedule.list", e))?;
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
        for url in &urls {
            validate_url(url).map_err(|e| invalid_params(e.to_string()))?;
        }
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
        .map_err(|e| logged_internal_error("refresh.schedule.create", e))?;
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
            .map_err(|e| logged_internal_error("refresh.schedule.delete", e))?;
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
                .map_err(|e| logged_internal_error("refresh.schedule.enable", e))?
        } else {
            refresh_service::refresh_schedule_disable(self.cfg.as_ref(), &name)
                .await
                .map_err(|e| logged_internal_error("refresh.schedule.disable", e))?
        };
        Ok(AxonToolResponse::ok(
            "refresh",
            "schedule",
            serde_json::json!({ "name": name, "enabled": enabled, "updated": updated }),
        ))
    }
}
