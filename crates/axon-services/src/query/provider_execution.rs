//! Durable provider scheduling for foreground read operations.

use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use axon_core::config::Config;
use axon_embedding::provider::EmbeddingProvider;
use axon_vectors::store::VectorStore;

use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use crate::reserved_call::{self, ProviderCallContext};

pub(super) struct ReadExecution {
    runtime: Arc<TargetLocalSourceRuntime>,
    descriptor: Option<JobDescriptor>,
    job_id: JobId,
}

impl ReadExecution {
    pub(super) async fn begin(
        ctx: &ServiceContext,
        cfg: &Config,
        operation: OperationKind,
        request: serde_json::Value,
        auth_snapshot: Option<AuthSnapshot>,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let runtime = if let Some(runtime) = ctx.target_local_source_runtime() {
            Arc::new(runtime.clone())
        } else {
            let store = ctx
                .job_store()
                .ok_or_else(|| -> Box<dyn Error + Send + Sync> {
                    "unified job store is unavailable for scheduled read execution".into()
                })?;
            let pool = ctx
                .sqlite_pool()
                .ok_or_else(|| -> Box<dyn Error + Send + Sync> {
                    "SQLite scheduler pool is unavailable for scheduled read execution".into()
                })?;
            Arc::new(
                TargetLocalSourceRuntime::from_config(cfg, store, (*pool).clone())
                    .await
                    .map_err(|error| -> Box<dyn Error + Send + Sync> { error })?,
            )
        };

        let descriptor = begin_read_descriptor(ctx, operation, request, auth_snapshot).await?;
        let job_id = descriptor
            .as_ref()
            .map(|descriptor| descriptor.job_id)
            .unwrap_or_else(|| JobId::new(uuid::Uuid::new_v4()));
        Ok(Self {
            runtime,
            descriptor,
            job_id,
        })
    }

    pub(super) fn scheduled_vectors(&self) -> Arc<dyn VectorStore> {
        Arc::new(ScheduledVectorStore {
            runtime: Arc::clone(&self.runtime),
            base: self.call_context("vector"),
        })
    }

    pub(super) fn scheduled_embedding(&self) -> Arc<dyn EmbeddingProvider> {
        Arc::new(ScheduledEmbeddingProvider {
            runtime: Arc::clone(&self.runtime),
            base: self.call_context("embedding"),
        })
    }

    pub(super) fn embedding_provider_id(&self) -> ProviderId {
        self.runtime.embedding_provider_id.clone()
    }

    pub(super) fn embedding_model(&self) -> String {
        self.runtime.embedding_model.clone()
    }

    pub(super) fn embedding_dimensions(&self) -> u32 {
        self.runtime.embedding_dimensions
    }

    fn call_context(&self, operation_id: &str) -> ProviderCallContext {
        ProviderCallContext::new(self.job_id, 1, None, JobPriority::Interactive, operation_id)
    }

    pub(super) async fn vector_operation<T, F, Fut>(
        &self,
        operation_id: &str,
        operation: F,
    ) -> Result<T, ApiError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, ApiError>>,
    {
        reserved_call::vector_operation(&self.runtime, self.call_context(operation_id), operation)
            .await
    }

    pub(super) async fn finish<T, E>(&self, ctx: &ServiceContext, result: &Result<T, E>)
    where
        E: std::fmt::Display,
    {
        let Some(descriptor) = &self.descriptor else {
            return;
        };
        let outcome = result.as_ref().map(|_| ()).map_err(ToString::to_string);
        if let Err(error) = crate::jobs::complete_operation_job(ctx, descriptor, outcome).await {
            tracing::warn!(job_id = %descriptor.job_id.0, %error, "failed to mark interactive read job terminal");
        }
    }
}

async fn begin_read_descriptor(
    ctx: &ServiceContext,
    operation: OperationKind,
    request: serde_json::Value,
    auth_snapshot: Option<AuthSnapshot>,
) -> Result<Option<JobDescriptor>, Box<dyn Error + Send + Sync>> {
    if ctx.job_store().is_none() {
        return Ok(None);
    }
    let descriptor = crate::jobs::enqueue_operation_with_context(
        ctx,
        operation,
        JobExecutionMode::Foreground,
        request,
        JobPriority::Interactive,
        auth_snapshot.unwrap_or_else(|| AuthSnapshot::scheduler_bookkeeping("runtime")),
    )
    .await?;
    if let Some(descriptor) = &descriptor
        && let Err(error) = crate::jobs::start_operation_job(ctx, descriptor).await
    {
        tracing::warn!(job_id = %descriptor.job_id.0, %error, "failed to mark interactive read job running");
    }
    Ok(descriptor)
}

#[derive(Clone)]
struct ScheduledEmbeddingProvider {
    runtime: Arc<TargetLocalSourceRuntime>,
    base: ProviderCallContext,
}

#[async_trait]
impl EmbeddingProvider for ScheduledEmbeddingProvider {
    async fn embed(&self, mut batch: EmbeddingBatch) -> Result<EmbeddingResult, ApiError> {
        batch.job_id = self.base.job_id;
        batch.priority = self.base.priority;
        let mut context = self.base.clone();
        context.operation_id = format!("{}:{}", self.base.operation_id, batch.batch_id.0);
        reserved_call::embed(&self.runtime, context, batch).await
    }

    async fn capabilities(&self) -> Result<ProviderCapability, ApiError> {
        self.runtime.embedding_provider.capabilities().await
    }
}

#[derive(Clone)]
struct ScheduledVectorStore {
    runtime: Arc<TargetLocalSourceRuntime>,
    base: ProviderCallContext,
}

impl ScheduledVectorStore {
    fn context(&self, operation: &str) -> ProviderCallContext {
        let mut context = self.base.clone();
        context.operation_id = format!(
            "{}:{operation}:{}",
            self.base.operation_id,
            uuid::Uuid::new_v4()
        );
        context
    }
}

#[async_trait]
impl VectorStore for ScheduledVectorStore {
    async fn ensure_collection(&self, spec: CollectionSpec) -> Result<(), ApiError> {
        reserved_call::ensure_collection(&self.runtime, self.context("ensure"), spec).await
    }

    async fn upsert(&self, batch: VectorPointBatch) -> Result<VectorStoreWriteResult, ApiError> {
        reserved_call::upsert(&self.runtime, self.context("upsert"), batch).await
    }

    async fn mark_generation_committed(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<VectorStoreWriteResult, ApiError> {
        reserved_call::mark_generation_committed(
            &self.runtime,
            self.context("commit"),
            collection,
            source_id,
            generation,
        )
        .await
    }

    async fn mark_unchanged_items_committed(
        &self,
        collection: String,
        source_id: SourceId,
        previous_generation: SourceGenerationId,
        committed_generation: SourceGenerationId,
        source_item_keys: Vec<SourceItemKey>,
    ) -> Result<VectorStoreWriteResult, ApiError> {
        reserved_call::mark_unchanged_items_committed(
            &self.runtime,
            self.context("carry-forward"),
            collection,
            source_id,
            previous_generation,
            committed_generation,
            source_item_keys,
        )
        .await
    }

    async fn retire_generation(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
        retired_epoch: SourceGenerationId,
    ) -> Result<VectorStoreWriteResult, ApiError> {
        reserved_call::retire_generation(
            &self.runtime,
            self.context("retire"),
            collection,
            source_id,
            generation,
            retired_epoch,
        )
        .await
    }

    async fn delete(
        &self,
        selector: VectorDeleteSelector,
    ) -> Result<VectorStoreDeleteResult, ApiError> {
        reserved_call::delete_vectors(&self.runtime, self.context("delete"), selector).await
    }

    async fn search(&self, request: VectorSearchRequest) -> Result<VectorSearchResult, ApiError> {
        reserved_call::search_vectors(&self.runtime, self.context("search"), request).await
    }

    async fn capabilities(&self) -> Result<ProviderCapability, ApiError> {
        self.runtime.vector_store.capabilities().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn foreground_read_descriptor_persists_exact_caller_snapshot() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut cfg = Config::default();
        cfg.sqlite_path = temp.path().join("jobs.db");
        let ctx = ServiceContext::new(Arc::new(cfg))
            .await
            .expect("enqueue-only service context");
        let snapshot = AuthSnapshot::panel("panel-policy-v1");

        let descriptor = begin_read_descriptor(
            &ctx,
            OperationKind::Query,
            serde_json::json!({ "query": "snapshot proof" }),
            Some(snapshot.clone()),
        )
        .await
        .expect("begin foreground read")
        .expect("foreground read job descriptor");

        let pool = ctx.sqlite_pool().expect("sqlite pool");
        let stored_json: String =
            sqlx::query_scalar("SELECT auth_snapshot_json FROM jobs WHERE job_id = ?")
                .bind(descriptor.job_id.0.to_string())
                .fetch_one(pool.as_ref())
                .await
                .expect("stored auth snapshot");
        let stored: AuthSnapshot =
            serde_json::from_str(&stored_json).expect("deserialize stored auth snapshot");

        assert_eq!(stored, snapshot);
        assert_eq!(
            stored.granted_scopes,
            vec![AuthScope::Read, AuthScope::Write]
        );
        assert!(!stored.granted_scopes.contains(&AuthScope::Admin));
        assert!(!stored.granted_scopes.contains(&AuthScope::Local));
        assert!(!stored.granted_scopes.contains(&AuthScope::Execute));
    }

    #[tokio::test]
    async fn foreground_read_descriptor_uses_bookkeeping_only_without_caller_snapshot() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut cfg = Config::default();
        cfg.sqlite_path = temp.path().join("jobs.db");
        let ctx = ServiceContext::new(Arc::new(cfg))
            .await
            .expect("enqueue-only service context");

        let descriptor = begin_read_descriptor(
            &ctx,
            OperationKind::Retrieve,
            serde_json::json!({ "url": "https://example.test/" }),
            None,
        )
        .await
        .expect("begin foreground retrieve")
        .expect("foreground retrieve job descriptor");

        let pool = ctx.sqlite_pool().expect("sqlite pool");
        let stored_json: String =
            sqlx::query_scalar("SELECT auth_snapshot_json FROM jobs WHERE job_id = ?")
                .bind(descriptor.job_id.0.to_string())
                .fetch_one(pool.as_ref())
                .await
                .expect("stored auth snapshot");
        let stored: AuthSnapshot =
            serde_json::from_str(&stored_json).expect("deserialize stored auth snapshot");

        assert_eq!(stored.caller_id.as_deref(), Some("axon-scheduler"));
        assert_eq!(stored.granted_scopes, vec![AuthScope::Read]);
    }
}
