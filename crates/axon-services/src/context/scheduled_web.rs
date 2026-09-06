//! Durable scheduling wrappers for injected web fetch/render providers.

use std::sync::Arc;

use async_trait::async_trait;
use axon_adapters::boundary::{FetchProvider, RenderProvider};
use axon_api::source::{
    ApiError, FetchRequest, FetchedResource, ProviderCapability, RenderRequest, RenderedResource,
    read_provider_execution_metadata,
};
use axon_error::ErrorStage;
use axon_jobs::scheduler::{
    ProviderScheduler, ReservationRequest, ReservedCallError, SchedulerError, call_reserved,
};

struct FetchLane;
struct RenderLane;

#[derive(Clone)]
pub(super) struct ScheduledFetchProvider {
    inner: Arc<dyn FetchProvider>,
    scheduler: Arc<ProviderScheduler>,
    provider_id: &'static str,
}

impl ScheduledFetchProvider {
    pub(super) fn new(
        inner: Arc<dyn FetchProvider>,
        scheduler: Arc<ProviderScheduler>,
        provider_id: &'static str,
    ) -> Self {
        Self {
            inner,
            scheduler,
            provider_id,
        }
    }
}

#[async_trait]
impl FetchProvider for ScheduledFetchProvider {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchedResource, ApiError> {
        let Some(execution) = read_provider_execution_metadata(&request.metadata) else {
            return self.inner.fetch(request).await;
        };
        let reservation = reservation_request(execution, "fetch");
        let inner = Arc::clone(&self.inner);
        map_reserved(
            call_reserved::<FetchLane, _, _, _, _>(
                &self.scheduler,
                reservation,
                move |_| async move { inner.fetch(request).await },
            )
            .await,
            ErrorStage::Fetching,
            self.provider_id,
        )
    }

    async fn capabilities(&self) -> Result<ProviderCapability, ApiError> {
        self.inner.capabilities().await
    }
}

#[derive(Clone)]
pub(super) struct ScheduledRenderProvider {
    inner: Arc<dyn RenderProvider>,
    scheduler: Arc<ProviderScheduler>,
    provider_id: &'static str,
}

impl ScheduledRenderProvider {
    pub(super) fn new(
        inner: Arc<dyn RenderProvider>,
        scheduler: Arc<ProviderScheduler>,
        provider_id: &'static str,
    ) -> Self {
        Self {
            inner,
            scheduler,
            provider_id,
        }
    }
}

#[async_trait]
impl RenderProvider for ScheduledRenderProvider {
    async fn render(&self, request: RenderRequest) -> Result<RenderedResource, ApiError> {
        let Some(execution) = read_provider_execution_metadata(&request.metadata) else {
            return self.inner.render(request).await;
        };
        let reservation = reservation_request(execution, "render");
        let inner = Arc::clone(&self.inner);
        map_reserved(
            call_reserved::<RenderLane, _, _, _, _>(
                &self.scheduler,
                reservation,
                move |_| async move { inner.render(request).await },
            )
            .await,
            ErrorStage::Rendering,
            self.provider_id,
        )
    }

    async fn capabilities(&self) -> Result<ProviderCapability, ApiError> {
        self.inner.capabilities().await
    }
}

fn reservation_request(
    execution: axon_api::source::ProviderExecutionMetadata,
    operation: &str,
) -> ReservationRequest {
    ReservationRequest {
        job_id: execution.job_id,
        stage_id: None,
        attempt: execution.attempt,
        fence: format!(
            "{}:{}:{operation}:{}",
            execution.job_id.0,
            execution.attempt,
            uuid::Uuid::new_v4()
        ),
        priority: execution.priority,
        units: 1,
    }
}

fn map_reserved<T>(
    result: Result<T, ReservedCallError<ApiError>>,
    stage: ErrorStage,
    provider_id: &str,
) -> Result<T, ApiError> {
    match result {
        Ok(value) => Ok(value),
        Err(ReservedCallError::Provider(error)) => Err(error),
        Err(ReservedCallError::Scheduler(error)) => Err(scheduler_error(error, stage, provider_id)),
    }
}

fn scheduler_error(error: SchedulerError, stage: ErrorStage, provider_id: &str) -> ApiError {
    let code = match error {
        SchedulerError::RequestTooLarge => "provider.scheduler.request_too_large",
        SchedulerError::InvalidConfig(_) => "provider.scheduler.invalid_config",
        SchedulerError::QueueFull => "provider.scheduler.queue_full",
        SchedulerError::WaitTimeout => "provider.scheduler.wait_timeout",
        SchedulerError::StaleFence => "provider.scheduler.stale_fence",
        SchedulerError::Queued => "provider.scheduler.queued",
        SchedulerError::Database(_) => "provider.scheduler.database",
        SchedulerError::DatabaseState(_) => "provider.scheduler.database_state",
        SchedulerError::RollbackFailed { .. } => "provider.scheduler.rollback_failed",
    };
    ApiError::new(code, stage, error.to_string()).with_provider_id(provider_id)
}
