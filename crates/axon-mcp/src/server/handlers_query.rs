#[path = "handlers_query/brand_diff.rs"]
mod brand_diff;
#[path = "handlers_query/query.rs"]
mod query;

use super::AxonMcpServer;
use super::common::{
    InlineHint, internal_error, invalid_params, logged_internal_error, map_render_mode,
    respond_with_mode, slugify, to_map_options, to_retrieve_options, to_search_options,
    validate_mcp_collection, validate_mcp_url,
};
use crate::schema::{
    AskRequest, AxonToolResponse, EndpointsRequest, EvaluateRequest, MapRequest, ResearchRequest,
    RetrieveRequest, SearchRequest, SuggestRequest, SummarizeRequest,
};
use axon_core::config::ConfigOverrides;
use axon_services::{
    endpoints as endpoints_svc, map as map_svc, query as query_svc, search as search_svc,
    search_crawl as search_crawl_svc, summarize as summarize_svc,
};
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(super) async fn handle_retrieve(
        &self,
        req: RetrieveRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let target = req
            .url
            .ok_or_else(|| invalid_params("url is required for retrieve"))?;
        let response_mode = req.response_mode;
        let opts = to_retrieve_options(req.max_points, req.cursor.clone(), req.token_budget);
        let collection = req
            .collection
            .as_deref()
            .map(validate_mcp_collection)
            .transpose()?;
        let cfg = self.cfg.apply_overrides(&ConfigOverrides {
            collection,
            since: req.since,
            before: req.before,
            ..ConfigOverrides::default()
        });

        let result = query_svc::retrieve(&cfg, &target, opts)
            .await
            .map_err(|e| logged_internal_error(&format!("retrieve '{target}'"), e.as_ref()))?;
        respond_with_mode(
            "retrieve",
            "retrieve",
            response_mode,
            &format!("retrieve-{}", slugify(&target, 56)),
            serde_json::to_value(result)
                .map_err(|e| internal_error(format!("serialize retrieve result: {e}")))?,
            InlineHint::Document,
        )
        .await
    }

    pub(super) async fn handle_map(&self, req: MapRequest) -> Result<AxonToolResponse, ErrorData> {
        let url = req
            .url
            .ok_or_else(|| invalid_params("url is required for map"))?;
        validate_mcp_url(&url)?;
        let response_mode = req.response_mode;
        let map_opts = to_map_options(req.limit, req.offset);
        let result = map_svc::discover(self.cfg.as_ref(), &url, map_opts, None)
            .await
            .map_err(|e| logged_internal_error(&format!("map '{url}'"), e.as_ref()))?;
        respond_with_mode(
            "map",
            "map",
            response_mode,
            &format!("map-{}", slugify(&url, 56)),
            serde_json::json!({
                "url": result.url,
                "urls": result.urls,
                "mapped_urls": result.returned_url_count,
                "total": result.total,
                "total_urls": result.total,
                "limit": map_opts.limit,
                "offset": map_opts.offset,
                "sitemap_urls": result.sitemap_urls,
                "pages_seen": result.pages_seen,
                "thin_pages": result.thin_pages,
                "elapsed_ms": result.elapsed_ms,
                "map_source": result.map_source,
                "warning": result.warning,
            }),
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_endpoints(
        &self,
        req: EndpointsRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let url = req
            .url
            .ok_or_else(|| invalid_params("url is required for endpoints"))?;
        validate_mcp_url(&url)?;
        let response_mode = req.response_mode;
        let mut options = endpoints_svc::options_from_config(self.cfg.as_ref());
        if let Some(value) = req.include_bundles {
            options.include_bundles = value;
        }
        if let Some(value) = req.first_party_only {
            options.first_party_only = value;
        }
        if let Some(value) = req.unique_only {
            options.unique_only = value;
        }
        if let Some(value) = req.max_scripts {
            options.max_scripts = value;
        }
        if let Some(value) = req.max_scan_bytes {
            options.max_scan_bytes = value;
        }
        if let Some(value) = req.verify {
            options.verify = value;
        }
        if let Some(value) = req.capture_network {
            options.capture_network = value;
        }
        if let Some(value) = req.probe_rpc {
            options.probe_rpc = value;
        }
        if let Some(value) = req.probe_rpc_subdomains {
            options.probe_rpc_subdomains = value;
        }
        let result = endpoints_svc::discover(self.cfg.as_ref(), &url, options, None)
            .await
            .map_err(|e| logged_internal_error(&format!("endpoints '{url}'"), e.as_ref()))?;
        respond_with_mode(
            "endpoints",
            "endpoints",
            response_mode,
            &format!("endpoints-{}", slugify(&url, 56)),
            serde_json::to_value(result)
                .map_err(|e| internal_error(format!("serialize endpoints result: {e}")))?,
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_search(
        &self,
        req: SearchRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let query = req
            .query
            .ok_or_else(|| invalid_params("query is required for search"))?;
        let response_mode = req.response_mode;
        let opts = to_search_options(
            req.limit,
            req.offset,
            req.search_time_range,
            self.cfg.search_limit,
        );
        if self.cfg.tavily_api_key.is_empty() && self.cfg.searxng_url.is_empty() {
            return Err(internal_error(
                "search requires AXON_SEARXNG_URL or TAVILY_API_KEY",
            ));
        }
        let (limit, offset) = (opts.limit, opts.offset);
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("search.context", e.as_ref()))?;
        let result =
            search_crawl_svc::search_and_crawl(self.cfg.as_ref(), &service_context, &query, opts)
                .await
                .map_err(|e| logged_internal_error(&format!("search '{query}'"), e.as_ref()))?;

        respond_with_mode(
            "search",
            "search",
            response_mode,
            &format!("search-{}", slugify(&query, 56)),
            serde_json::json!({
                "query": query,
                "limit": limit,
                "offset": offset,
                "results": result.results,
                "auto_crawl_status": result.auto_crawl_status,
                "crawl_jobs": result.crawl_jobs,
                "crawl_jobs_rejected": result.crawl_rejected,
            }),
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_evaluate(
        &self,
        req: EvaluateRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let query = req
            .query
            .ok_or_else(|| invalid_params("query is required for evaluate"))?;
        let response_mode = req.response_mode;

        let mut cfg = self.cfg.as_ref().clone();
        if let Some(diagnostics) = req.diagnostics {
            cfg.ask_diagnostics = diagnostics;
        }
        if let Some(retrieval_ab) = req.retrieval_ab {
            cfg.evaluate_retrieval_ab = retrieval_ab;
        }
        if let Some(collection) = req.collection {
            cfg.collection = validate_mcp_collection(&collection)?;
        }
        if let Some(since) = req.since {
            cfg.since = Some(since);
        }
        if let Some(before) = req.before {
            cfg.before = Some(before);
        }
        if let Some(enabled) = req.hybrid_search {
            cfg.hybrid_search_enabled = enabled;
        }

        // The RMCP `#[tool]` wrapper requires a `Send` future. The evaluate
        // service currently carries `Box<dyn Error>` through `tokio::try_join!`
        // in the vector pipeline, making direct `.await` non-Send at this
        // boundary. Keep that non-Send implementation isolated from the MCP
        // tool future until the evaluate pipeline error type is widened.
        //
        // Issue #298: the RAG-retrieval half now runs through `axon-retrieval`
        // inside `query_svc::evaluate`, so the read-plane `ServiceContext` is
        // resolved here and moved into the isolated runtime (it is `Send`).
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| internal_error(format!("service context: {e}")))?;
        let query_for_task = query.clone();
        let result = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
            runtime.block_on(async { query_svc::evaluate(&ctx, &cfg, &query_for_task).await })
        })
        .await
        .map_err(|e| {
            tracing::error!("join evaluate task: {e}");
            internal_error(format!("evaluate '{query}' failed"))
        })?
        .map_err(|e| logged_internal_error(&format!("evaluate '{query}'"), e.as_ref()))?;

        respond_with_mode(
            "evaluate",
            "evaluate",
            response_mode,
            &format!("evaluate-{}", slugify(&query, 56)),
            serde_json::to_value(result)
                .map_err(|e| internal_error(format!("serialize evaluate result: {e}")))?,
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_suggest(
        &self,
        req: SuggestRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        let mut cfg = self.cfg.as_ref().clone();
        if let Some(collection) = req.collection {
            cfg.collection = validate_mcp_collection(&collection)?;
        }
        if let Some(limit) = req.limit {
            cfg.search_limit = limit.clamp(1, 100);
        }
        let focus = req.focus;
        let result = query_svc::suggest(&cfg, focus.as_deref())
            .await
            .map_err(|e| logged_internal_error("suggest", e.as_ref()))?;

        respond_with_mode(
            "suggest",
            "suggest",
            response_mode,
            "suggestions",
            serde_json::to_value(result)
                .map_err(|e| internal_error(format!("serialize suggest result: {e}")))?,
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_research(
        &self,
        req: ResearchRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        if self.cfg.tavily_api_key.is_empty() && self.cfg.searxng_url.is_empty() {
            return Err(internal_error(
                "research requires AXON_SEARXNG_URL or TAVILY_API_KEY",
            ));
        }
        let query = req
            .query
            .ok_or_else(|| invalid_params("query is required for research"))?;
        let response_mode = req.response_mode;
        let opts = to_search_options(
            req.limit,
            req.offset,
            req.search_time_range,
            self.cfg.search_limit,
        );

        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("research.context", e.as_ref()))?;
        let result = search_svc::research_with_context(
            self.cfg.as_ref(),
            &service_context,
            &query,
            opts,
            None,
        )
        .await
        .map_err(|e| logged_internal_error(&format!("research '{query}'"), e.as_ref()))?;

        let payload_json = serde_json::to_value(&result.payload)
            .map_err(|e| internal_error(format!("research payload serialization failed: {e}")))?;

        respond_with_mode(
            "research",
            "research",
            response_mode,
            &format!("research-{}", slugify(&query, 56)),
            payload_json,
            InlineHint::Fields(&["summary"]),
        )
        .await
    }

    pub(super) async fn handle_ask(&self, req: AskRequest) -> Result<AxonToolResponse, ErrorData> {
        let query = req
            .query
            .ok_or_else(|| invalid_params("query is required for ask"))?;
        let response_mode = req.response_mode;

        let collection = req
            .collection
            .as_deref()
            .map(validate_mcp_collection)
            .transpose()?;
        let cfg = axon_services::transport::apply_ask_overrides(
            self.cfg.as_ref(),
            axon_services::transport::AskTransportOverrides {
                collection,
                since: req.since,
                before: req.before,
                diagnostics: req.diagnostics,
                explain: req.explain,
                hybrid_search: req.hybrid_search,
                ask_chunk_limit: req.ask_chunk_limit,
                ask_full_docs: req.ask_full_docs,
                ask_max_context_chars: req.ask_max_context_chars,
                ask_hybrid_candidates: req.ask_hybrid_candidates,
                ask_min_relevance_score: req.ask_min_relevance_score,
                ask_doc_chunk_limit: req.ask_doc_chunk_limit,
                ask_doc_fetch_concurrency: req.ask_doc_fetch_concurrency,
                ask_backfill_chunks: req.ask_backfill_chunks,
                ask_candidate_limit: req.ask_candidate_limit,
                ask_min_citations_nontrivial: req.ask_min_citations_nontrivial,
                ask_authoritative_domains: req.ask_authoritative_domains,
                ask_authoritative_boost: req.ask_authoritative_boost,
            },
        );

        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| internal_error(format!("service context: {e}")))?;
        let result = query_svc::ask(&ctx, &cfg, &query, None)
            .await
            .map_err(|e| logged_internal_error(&format!("ask '{query}'"), e.as_ref()))?;

        respond_with_mode(
            "ask",
            "ask",
            response_mode,
            &format!("ask-{}", slugify(&query, 56)),
            serde_json::to_value(result)
                .map_err(|e| internal_error(format!("serialize ask result: {e}")))?,
            InlineHint::Fields(&["answer"]),
        )
        .await
    }

    pub(super) async fn handle_summarize(
        &self,
        req: SummarizeRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let urls = {
            let collected = axon_services::action_api::collect_unique_urls(req.url, req.urls);
            if collected.is_empty() {
                return Err(invalid_params("url or urls is required for summarize"));
            }
            collected
        };
        for url in &urls {
            validate_mcp_url(url)?;
        }
        let response_mode = req.response_mode;
        let cfg = self.cfg.apply_overrides(&ConfigOverrides {
            render_mode: req.render_mode.map(map_render_mode),
            root_selector: req.root_selector,
            exclude_selector: req.exclude_selector,
            ..ConfigOverrides::default()
        });

        let result = summarize_svc::summarize(&cfg, &urls, None)
            .await
            .map_err(|e| logged_internal_error("summarize", e.as_ref()))?;

        respond_with_mode(
            "summarize",
            "summarize",
            response_mode,
            &format!("summarize-{}", slugify(&urls.join("-"), 56)),
            serde_json::to_value(result)
                .map_err(|e| internal_error(format!("serialize summarize result: {e}")))?,
            InlineHint::Fields(&["summary"]),
        )
        .await
    }
}
