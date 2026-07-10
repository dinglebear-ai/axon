//! `QueryService` — semantic vector search.
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §QueryService. Wraps `crate::query::query(ctx, cfg, text, opts)`,
//! mirroring the MCP `handle_query` handler
//! (`crates/axon-mcp/src/server/handlers_query/query.rs`): `limit`/`offset`
//! go through `crate::transport::pagination` (same default/cap as the
//! transport) and `collection`/`since`/`before`/`hybrid_search` are applied
//! onto the config via `ConfigOverrides` before the free function call, so no
//! `QueryRequest` field is silently dropped.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::mcp_schema::QueryRequest;
use axon_api::result::{QueryHit, QueryResult};
use axon_core::config::ConfigOverrides;

use crate::context::ServiceContext;

#[async_trait]
pub trait QueryService: Send + Sync {
    async fn query(&self, request: QueryRequest) -> anyhow::Result<QueryResult>;
}

pub struct QueryServiceImpl {
    ctx: Arc<ServiceContext>,
}

impl QueryServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl QueryService for QueryServiceImpl {
    async fn query(&self, request: QueryRequest) -> anyhow::Result<QueryResult> {
        let text = request.query.clone().unwrap_or_default();
        let opts = crate::transport::pagination(
            request.limit,
            request.offset,
            self.ctx.cfg().search_limit,
        );
        let cfg = self.ctx.cfg().apply_overrides(&ConfigOverrides {
            collection: request.collection,
            since: request.since,
            before: request.before,
            hybrid_search_enabled: request.hybrid_search,
            ..ConfigOverrides::default()
        });
        crate::query::query(&self.ctx, &cfg, &text, opts)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

/// Deterministic in-memory fake covering `QueryService::query`.
#[derive(Default)]
pub struct FakeQueryService {
    hits: Mutex<Vec<QueryHit>>,
}

impl FakeQueryService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed(&self, hit: QueryHit) {
        self.hits.lock().unwrap().push(hit);
    }
}

#[async_trait]
impl QueryService for FakeQueryService {
    async fn query(&self, request: QueryRequest) -> anyhow::Result<QueryResult> {
        let limit = request.limit.unwrap_or(10);
        let results = self
            .hits
            .lock()
            .unwrap()
            .iter()
            .take(limit)
            .cloned()
            .collect();
        Ok(QueryResult { results })
    }
}

#[cfg(test)]
#[path = "query_service_tests.rs"]
mod tests;
