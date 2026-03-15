use super::AxonMcpServer;
use super::common::{
    invalid_params, logged_internal_error, map_render_mode, map_scrape_format, paginate_vec,
    parse_offset, respond_with_mode, slugify, to_map_options, to_pagination, to_retrieve_options,
    to_search_options,
};
use crate::crates::mcp::schema::{
    AskRequest, AxonToolResponse, MapRequest, QueryRequest, ResearchRequest, RetrieveRequest,
    ScrapeRequest, SearchRequest,
};
use crate::crates::services::{
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
        let result = query_svc::query(self.cfg.as_ref(), &query, pagination)
            .await
            .map_err(|e| logged_internal_error("operation", e))?;

        respond_with_mode(
            "query",
            "query",
            response_mode,
            &format!("query-{}", slugify(&query, 56)),
            serde_json::json!({
                "query": query,
                "limit": limit,
                "offset": offset,
                "results": result.results,
            }),
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
        let result = query_svc::retrieve(self.cfg.as_ref(), &target, opts)
            .await
            .map_err(|e| logged_internal_error("operation", e))?;
        // chunks is a Vec<Value> of 0 or 1 items; the actual Qdrant point count
        // lives inside result.chunks[0]["chunk_count"], not in Vec::len().
        let chunk_count = result
            .chunks
            .first()
            .and_then(|c| c.get("chunk_count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let content = result
            .chunks
            .first()
            .and_then(|c| c.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        respond_with_mode(
            "retrieve",
            "retrieve",
            response_mode,
            &format!("retrieve-{}", slugify(&target, 56)),
            serde_json::json!({
                "url": target,
                "chunks": chunk_count,
                "content": content,
            }),
        )
        .await
    }

    pub(super) async fn handle_map(&self, req: MapRequest) -> Result<AxonToolResponse, ErrorData> {
        let url = req
            .url
            .ok_or_else(|| invalid_params("url is required for map"))?;
        let response_mode = req.response_mode;
        let map_opts = to_map_options(req.limit, req.offset);
        let (limit, offset) = (map_opts.limit, map_opts.offset);
        let result = map_svc::discover(self.cfg.as_ref(), &url, map_opts, None)
            .await
            .map_err(|e| logged_internal_error("operation", e))?;
        let payload = result.payload;
        let urls = payload["urls"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| v.as_str().map(ToString::to_string))
            .collect::<Vec<_>>();
        // limit=0 means "no limit" (return all); positive values paginate normally.
        let paged_urls = if limit == 0 {
            urls.clone()
        } else {
            paginate_vec(&urls, offset, limit)
        };
        respond_with_mode(
            "map",
            "map",
            response_mode,
            &format!("map-{}", slugify(&url, 56)),
            serde_json::json!({
                "url": url,
                "pages_seen": payload["pages_seen"].as_u64().unwrap_or(0),
                "elapsed_ms": payload["elapsed_ms"].as_u64().unwrap_or(0),
                "thin_pages": payload["thin_pages"].as_u64().unwrap_or(0),
                "limit": limit,
                "offset": offset,
                "total_urls": urls.len(),
                "urls": paged_urls,
            }),
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
            return Err(invalid_params("TAVILY_API_KEY is required for search"));
        }
        let result = search_svc::search(self.cfg.as_ref(), &query, opts, None)
            .await
            .map_err(|e| logged_internal_error("operation", e))?;

        respond_with_mode(
            "search",
            "search",
            response_mode,
            &format!("search-{}", slugify(&query, 56)),
            serde_json::json!({
                "query": query,
                "limit": opts.limit,
                "offset": opts.offset,
                "results": result.results,
            }),
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
        let response_mode = req.response_mode;

        let mut cfg = self.cfg.as_ref().clone();
        if let Some(rm) = req.render_mode {
            cfg.render_mode = map_render_mode(rm);
        }
        if let Some(fmt) = req.format {
            cfg.format = map_scrape_format(fmt);
        }
        if let Some(embed) = req.embed {
            cfg.embed = embed;
        }
        if let Some(sel) = req.root_selector {
            cfg.root_selector = Some(sel);
        }
        if let Some(sel) = req.exclude_selector {
            cfg.exclude_selector = Some(sel);
        }

        let result = scrape_svc::scrape(&cfg, &url)
            .await
            .map_err(|e| logged_internal_error("operation", e))?;
        respond_with_mode(
            "scrape",
            "scrape",
            response_mode,
            &format!("scrape-{}", slugify(&url, 56)),
            result.payload,
        )
        .await
    }

    pub(super) async fn handle_research(
        &self,
        req: ResearchRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        if self.cfg.tavily_api_key.is_empty() {
            return Err(invalid_params("TAVILY_API_KEY is required for research"));
        }
        if self.cfg.openai_base_url.is_empty() || self.cfg.openai_model.is_empty() {
            return Err(invalid_params(
                "OPENAI_BASE_URL and OPENAI_MODEL are required for research",
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

        let result = search_svc::research(self.cfg.as_ref(), &query, opts, None)
            .await
            .map_err(|e| logged_internal_error("operation", e))?;

        respond_with_mode(
            "research",
            "research",
            response_mode,
            &format!("research-{}", slugify(&query, 56)),
            result.payload,
        )
        .await
    }

    pub(super) async fn handle_ask(&self, req: AskRequest) -> Result<AxonToolResponse, ErrorData> {
        let query = req
            .query
            .ok_or_else(|| invalid_params("query is required for ask"))?;
        let response_mode = req.response_mode;

        let mut cfg = self.cfg.as_ref().clone();
        if let Some(graph) = req.graph {
            cfg.ask_graph = graph;
        }
        if let Some(diagnostics) = req.diagnostics {
            cfg.ask_diagnostics = diagnostics;
        }

        let result = query_svc::ask(&cfg, &query, None)
            .await
            .map_err(|e| logged_internal_error("operation", e))?;

        respond_with_mode(
            "ask",
            "ask",
            response_mode,
            &format!("ask-{}", slugify(&query, 56)),
            result.payload,
        )
        .await
    }
}
