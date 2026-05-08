use super::AxonMcpServer;
use super::common::{
    InlineHint, internal_error, invalid_params, logged_internal_error, map_render_mode,
    map_scrape_format, parse_offset, respond_with_mode, slugify, to_map_options, to_pagination,
    to_retrieve_options, to_search_options, validate_mcp_collection, validate_mcp_url,
};
use crate::core::config::ConfigOverrides;
use crate::mcp::schema::{
    AskRequest, AxonToolResponse, EvaluateRequest, MapRequest, QueryRequest, ResearchRequest,
    RetrieveRequest, ScrapeRequest, SearchRequest, SuggestRequest,
};
use crate::services::{
    map as map_svc, query as query_svc, scrape as scrape_svc, search as search_svc,
};
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(super) async fn handle_query(
        &self,
        req: QueryRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let query = req
            .query
            .ok_or_else(|| invalid_params("query is required for query"))?;
        let limit = req.limit.unwrap_or(self.cfg.search_limit).clamp(1, 500);
        let offset = parse_offset(req.offset);
        let response_mode = req.response_mode;
        let pagination = to_pagination(Some(limit), Some(offset), self.cfg.search_limit);

        let collection = req
            .collection
            .as_deref()
            .map(validate_mcp_collection)
            .transpose()?;
        let cfg = self.cfg.apply_overrides(&ConfigOverrides {
            collection,
            since: req.since,
            before: req.before,
            hybrid_search_enabled: req.hybrid_search,
            ..ConfigOverrides::default()
        });

        let result = query_svc::query(&cfg, &query, pagination)
            .await
            .map_err(|e| logged_internal_error(&format!("query '{query}'"), e.as_ref()))?;

        respond_with_mode(
            "query",
            "query",
            response_mode,
            &format!("query-{}", slugify(&query, 56)),
            serde_json::json!({
                "query": query,
                "limit": limit,
                "offset": offset,
                "results": serde_json::to_value(&result.results).map_err(|e| internal_error(format!("serialize query results: {e}")))?,
            }),
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_retrieve(
        &self,
        req: RetrieveRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let target = req
            .url
            .ok_or_else(|| invalid_params("url is required for retrieve"))?;
        let response_mode = req.response_mode;
        let opts = to_retrieve_options(req.max_points);
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
            serde_json::json!({
                "url": target,
                "chunks": result.chunk_count,
                "content": result.content,
            }),
            InlineHint::AlwaysPath,
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
        let (limit, offset) = (map_opts.limit, map_opts.offset);
        let result = map_svc::discover(self.cfg.as_ref(), &url, map_opts, None)
            .await
            .map_err(|e| logged_internal_error(&format!("map '{url}'"), e.as_ref()))?;
        // The service already applied offset/limit pagination.
        // `result.total` is the pre-pagination count; `result.urls` is the page slice.
        let total_urls = result.total;
        respond_with_mode(
            "map",
            "map",
            response_mode,
            &format!("map-{}", slugify(&url, 56)),
            serde_json::json!({
                "url": url,
                "pages_seen": result.pages_seen,
                "elapsed_ms": result.elapsed_ms,
                "thin_pages": result.thin_pages,
                "limit": limit,
                "offset": offset,
                "total_urls": total_urls,
                "urls": result.urls,
            }),
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
        if self.cfg.tavily_api_key.is_empty() {
            return Err(internal_error("TAVILY_API_KEY is required for search"));
        }
        let (limit, offset) = (opts.limit, opts.offset);
        let result = search_svc::search(self.cfg.as_ref(), &query, opts, None)
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
        let query_for_task = query.clone();
        let result = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("build evaluate runtime: {e}"))?;
            runtime.block_on(async {
                query_svc::evaluate(&cfg, &query_for_task)
                    .await
                    .map_err(|e| e.to_string())
            })
        })
        .await
        .map_err(|e| {
            tracing::error!("join evaluate task: {e}");
            internal_error(format!("evaluate '{query}' failed"))
        })?
        .map_err(|e| {
            tracing::error!("evaluate '{query}': {e}");
            internal_error(format!("evaluate '{query}' failed"))
        })?;

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

    pub(super) async fn handle_scrape(
        &self,
        req: ScrapeRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let url = req
            .url
            .ok_or_else(|| invalid_params("url is required for scrape"))?;
        validate_mcp_url(&url)?;
        let response_mode = req.response_mode;

        let cfg = self.cfg.apply_overrides(&ConfigOverrides {
            render_mode: req.render_mode.map(map_render_mode),
            format: req.format.map(map_scrape_format),
            embed: req.embed,
            root_selector: req.root_selector,
            exclude_selector: req.exclude_selector,
            ..ConfigOverrides::default()
        });

        let result = scrape_svc::scrape(&cfg, &url, None)
            .await
            .map_err(|e| logged_internal_error(&format!("scrape '{url}'"), e.as_ref()))?;
        respond_with_mode(
            "scrape",
            "scrape",
            response_mode,
            &format!("scrape-{}", slugify(&url, 56)),
            result.payload,
            InlineHint::AlwaysPath,
        )
        .await
    }

    pub(super) async fn handle_research(
        &self,
        req: ResearchRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        if self.cfg.tavily_api_key.is_empty() {
            return Err(internal_error("TAVILY_API_KEY is required for research"));
        }
        let backend = crate::services::llm_backend::LlmBackendConfig::from_config(&self.cfg);
        if let Err(err) = crate::services::llm_backend::headless::gemini::validate_config(&backend)
        {
            return Err(internal_error(format!(
                "Gemini headless is required for research: {err}"
            )));
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

        let result = search_svc::research(self.cfg.as_ref(), &query, opts, None)
            .await
            .map_err(|e| logged_internal_error(&format!("research '{query}'"), e.as_ref()))?;

        respond_with_mode(
            "research",
            "research",
            response_mode,
            &format!("research-{}", slugify(&query, 56)),
            result.payload,
            InlineHint::Fields(&["summary"]),
        )
        .await
    }

    pub(super) async fn handle_ask(&self, req: AskRequest) -> Result<AxonToolResponse, ErrorData> {
        let query = req
            .query
            .ok_or_else(|| invalid_params("query is required for ask"))?;
        let response_mode = req.response_mode;
        if req.graph == Some(true) {
            return Err(invalid_params(
                "graph retrieval is unavailable in this build; omit graph or set graph=false",
            ));
        }

        let collection = req
            .collection
            .as_deref()
            .map(validate_mcp_collection)
            .transpose()?;
        let cfg = self.cfg.apply_overrides(&ConfigOverrides {
            ask_graph: Some(false),
            ask_diagnostics: req.diagnostics,
            collection,
            since: req.since,
            before: req.before,
            hybrid_search_enabled: req.hybrid_search,
            ..ConfigOverrides::default()
        });

        let result = query_svc::ask(&cfg, &query, None)
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
}
