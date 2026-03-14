use super::AxonMcpServer;
use super::artifacts::{
    artifact_root, clean_artifact_files, client_context_name, delete_artifact_file, line_count,
    list_artifact_files, respond_with_mode, search_artifact_files, validate_artifact_path,
};
use super::common::{
    MCP_TOOL_SCHEMA_URI, invalid_params, logged_internal_error, parse_limit_usize, parse_offset,
    to_pagination,
};
use crate::crates::mcp::schema::{
    ArtifactsRequest, ArtifactsSubaction, AxonToolResponse, DoctorRequest, DomainsRequest,
    HelpRequest, SourcesRequest, StatsRequest,
};
use crate::crates::services::system;
use regex::Regex;
use rmcp::ErrorData;
use std::path::Path;

#[path = "handlers_system/screenshot.rs"]
mod screenshot;

// --- Private helpers for artifact inspection ---

impl AxonMcpServer {
    fn artifacts_grep_file(
        path: &Path,
        text: &str,
        pattern: &str,
        limit: usize,
        offset: usize,
        ctx: usize,
    ) -> Result<AxonToolResponse, ErrorData> {
        let re = Regex::new(pattern)
            .map_err(|e| invalid_params(format!("invalid regex pattern: {e}")))?;
        let lines: Vec<&str> = text.lines().collect();
        let matches: Vec<_> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| re.is_match(line))
            .skip(offset)
            .take(limit)
            .map(|(idx, line)| {
                let before = lines[idx.saturating_sub(ctx)..idx].to_vec();
                let after_end = (idx + ctx + 1).min(lines.len());
                let after = lines[(idx + 1)..after_end].to_vec();
                serde_json::json!({
                    "line": idx + 1,
                    "text": line,
                    "context_before": before,
                    "context_after": after,
                })
            })
            .collect();
        Ok(AxonToolResponse::ok(
            "artifacts",
            "grep",
            serde_json::json!({
                "path": path,
                "pattern": pattern,
                "context_lines": ctx,
                "limit": limit,
                "offset": offset,
                "matches": matches,
            }),
        ))
    }

    fn artifacts_read_file(
        path: &Path,
        text: &str,
        pattern: Option<&str>,
        full: bool,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<AxonToolResponse, ErrorData> {
        match (pattern, full) {
            (Some(pattern), _) => {
                let re = Regex::new(pattern)
                    .map_err(|e| invalid_params(format!("invalid regex pattern: {e}")))?;
                let limit = parse_limit_usize(limit, 200, 5_000);
                let offset = parse_offset(offset);
                let content: Vec<_> = text
                    .lines()
                    .enumerate()
                    .filter(|(_, line)| re.is_match(line))
                    .skip(offset)
                    .take(limit)
                    .map(|(idx, line)| serde_json::json!({ "line": idx + 1, "text": line }))
                    .collect();
                Ok(AxonToolResponse::ok(
                    "artifacts",
                    "read",
                    serde_json::json!({
                        "path": path,
                        "filter": "pattern",
                        "pattern": pattern,
                        "offset": offset,
                        "limit": limit,
                        "matches": content,
                    }),
                ))
            }
            (None, true) => {
                let limit = parse_limit_usize(limit, 2_000, 20_000);
                let offset = parse_offset(offset);
                let content = text
                    .lines()
                    .skip(offset)
                    .take(limit)
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(AxonToolResponse::ok(
                    "artifacts",
                    "read",
                    serde_json::json!({
                        "path": path,
                        "filter": "full",
                        "offset": offset,
                        "limit": limit,
                        "line_count": line_count(text),
                        "content": content,
                    }),
                ))
            }
            (None, false) => Err(invalid_params(
                "artifacts.read requires either \
                 pattern (filtered read) or full: true (complete content)",
            )),
        }
    }
}

// --- Public handlers ---

impl AxonMcpServer {
    pub(super) async fn handle_artifacts(
        &self,
        req: ArtifactsRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        match req.subaction {
            ArtifactsSubaction::List => {
                let limit = parse_limit_usize(req.limit, 50, 500);
                let offset = parse_offset(req.offset);
                let result = list_artifact_files(limit, offset).await?;
                respond_with_mode("artifacts", "list", response_mode, "artifacts-list", result)
                    .await
            }
            ArtifactsSubaction::Search => {
                let pattern = req
                    .pattern
                    .as_deref()
                    .ok_or_else(|| invalid_params("pattern is required for artifacts.search"))?;
                let limit = parse_limit_usize(req.limit, 25, 500);
                let result = search_artifact_files(pattern, limit).await?;
                respond_with_mode(
                    "artifacts",
                    "search",
                    response_mode,
                    "artifacts-search",
                    result,
                )
                .await
            }
            ArtifactsSubaction::Clean => {
                let max_age_hours = req.max_age_hours.ok_or_else(|| {
                    invalid_params(
                        "max_age_hours is required for artifacts.clean \
                         (e.g. 24 to target files older than 24 hours)",
                    )
                })?;
                // dry_run defaults to true — never delete without explicit opt-in
                let dry_run = req.dry_run.unwrap_or(true);
                let result = clean_artifact_files(max_age_hours, dry_run).await?;
                Ok(AxonToolResponse::ok("artifacts", "clean", result))
            }
            _ => self.handle_artifacts_path_op(req).await,
        }
    }

    async fn handle_artifacts_path_op(
        &self,
        req: ArtifactsRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let raw_path = req
            .path
            .as_deref()
            .ok_or_else(|| invalid_params("path is required for this artifacts operation"))?;

        if matches!(req.subaction, ArtifactsSubaction::Delete) {
            let path = validate_artifact_path(raw_path).await?;
            let bytes_freed = delete_artifact_file(&path).await?;
            return Ok(AxonToolResponse::ok(
                "artifacts",
                "delete",
                serde_json::json!({ "deleted": path, "bytes_freed": bytes_freed }),
            ));
        }

        // Head / Grep / Wc / Read — all need the file text
        let path = validate_artifact_path(raw_path).await?;
        let text = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| logged_internal_error("operation", e))?;

        match req.subaction {
            ArtifactsSubaction::Head => {
                let limit = parse_limit_usize(req.limit, 25, 500);
                let head = text.lines().take(limit).collect::<Vec<_>>().join("\n");
                Ok(AxonToolResponse::ok(
                    "artifacts",
                    "head",
                    serde_json::json!({
                        "path": path,
                        "limit": limit,
                        "line_count": line_count(&text),
                        "head": head,
                    }),
                ))
            }
            ArtifactsSubaction::Grep => {
                let pattern = req
                    .pattern
                    .as_deref()
                    .ok_or_else(|| invalid_params("pattern is required for artifacts.grep"))?;
                let limit = parse_limit_usize(req.limit, 25, 500);
                let offset = parse_offset(req.offset);
                let ctx = req.context_lines.unwrap_or(0).min(20);
                Self::artifacts_grep_file(&path, &text, pattern, limit, offset, ctx)
            }
            ArtifactsSubaction::Wc => Ok(AxonToolResponse::ok(
                "artifacts",
                "wc",
                serde_json::json!({
                    "path": path,
                    "bytes": text.len(),
                    "lines": line_count(&text),
                }),
            )),
            ArtifactsSubaction::Read => Self::artifacts_read_file(
                &path,
                &text,
                req.pattern.as_deref(),
                req.full.unwrap_or(false),
                req.limit,
                req.offset,
            ),
            // Already handled above
            ArtifactsSubaction::List
            | ArtifactsSubaction::Search
            | ArtifactsSubaction::Clean
            | ArtifactsSubaction::Delete => unreachable!(),
        }
    }

    pub(super) async fn handle_help(
        &self,
        req: HelpRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        respond_with_mode(
            "help",
            "help",
            req.response_mode,
            "help-actions",
            serde_json::json!({
                "tool": "axon",
                "actions": {
                    "status": [],
                    "help": [],
                    "scrape": ["scrape"],
                    "research": ["research"],
                    "ask": ["ask"],
                    "screenshot": ["screenshot"],
                    "crawl": ["start", "status", "cancel", "list", "cleanup", "clear", "recover"],
                    "extract": ["start", "status", "cancel", "list", "cleanup", "clear", "recover"],
                    "embed": ["start", "status", "cancel", "list", "cleanup", "clear", "recover"],
                    "ingest": ["start", "status", "cancel", "list", "cleanup", "clear", "recover"],
                    "refresh": ["start", "status", "cancel", "list", "cleanup", "clear", "recover", "schedule"],
                    "query": ["query"],
                    "retrieve": ["retrieve"],
                    "search": ["search"],
                    "map": ["map"],
                    "doctor": ["doctor"],
                    "domains": ["domains"],
                    "sources": ["sources"],
                    "stats": ["stats"],
                    "artifacts": ["head", "grep", "wc", "read", "list", "delete", "clean", "search"],
                    "elicit_demo": []
                },
                "resources": [
                    MCP_TOOL_SCHEMA_URI
                ],
                "defaults": {
                    "response_mode": "path",
                    "artifact_dir": artifact_root(),
                    "artifact_context": client_context_name()
                }
            }),
        )
        .await
    }

    pub(super) async fn handle_doctor(
        &self,
        req: DoctorRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        let result = system::doctor(self.cfg.as_ref())
            .await
            .map_err(|e| logged_internal_error("operation", e))?;
        respond_with_mode("doctor", "doctor", response_mode, "doctor", result.payload).await
    }

    pub(super) async fn handle_domains(
        &self,
        req: DomainsRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let pagination = to_pagination(req.limit.or(Some(25)), req.offset);
        let response_mode = req.response_mode;
        let result = system::domains(self.cfg.as_ref(), pagination)
            .await
            .map_err(|e| logged_internal_error("operation", e))?;
        let payload = serde_json::json!({
            "limit": result.limit,
            "offset": result.offset,
            "domains": result.domains.iter().map(|d| serde_json::json!({
                "domain": d.domain,
                "vectors": d.vectors,
            })).collect::<Vec<_>>(),
        });
        respond_with_mode("domains", "domains", response_mode, "domains", payload).await
    }

    pub(super) async fn handle_sources(
        &self,
        req: SourcesRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let pagination = to_pagination(req.limit.or(Some(25)), req.offset);
        let response_mode = req.response_mode;
        let result = system::sources(self.cfg.as_ref(), pagination)
            .await
            .map_err(|e| logged_internal_error("operation", e))?;
        let payload = serde_json::json!({
            "count": result.count,
            "limit": result.limit,
            "offset": result.offset,
            "urls": result.urls.iter().map(|(url, _chunks)| url).collect::<Vec<_>>(),
        });
        respond_with_mode("sources", "sources", response_mode, "sources", payload).await
    }

    pub(super) async fn handle_stats(
        &self,
        req: StatsRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        let result = system::stats(self.cfg.as_ref())
            .await
            .map_err(|e| logged_internal_error("operation", e))?;
        respond_with_mode("stats", "stats", response_mode, "stats", result.payload).await
    }
}
