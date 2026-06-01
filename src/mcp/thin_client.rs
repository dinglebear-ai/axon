use crate::cli::client::{ServerClient, ServerClientError};
use crate::core::config::Config;
use crate::mcp::schema::{
    AxonRequest, CrawlSubaction, EmbedSubaction, ExtractSubaction, IngestSubaction,
};
use serde_json::{Map, Value};
use std::fmt;

pub fn should_use_mcp_thin_client(cfg: &Config) -> bool {
    cfg.server_url.is_some() && !cfg.local_mode
}

#[derive(Debug)]
pub enum ThinClientError {
    MissingServerUrl,
    InvalidRequest(String),
    Server(ServerClientError),
}

impl fmt::Display for ThinClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingServerUrl => f.write_str("MCP thin client requires AXON_SERVER_URL"),
            Self::InvalidRequest(message) => f.write_str(message),
            Self::Server(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ThinClientError {}

pub async fn route_request(
    cfg: &Config,
    request: &AxonRequest,
) -> Result<Option<Value>, ThinClientError> {
    if !should_use_mcp_thin_client(cfg) {
        return Ok(None);
    }
    let server_url = cfg
        .server_url
        .clone()
        .ok_or(ThinClientError::MissingServerUrl)?;
    let client = ServerClient::new(server_url).map_err(ThinClientError::Server)?;
    let route = match request {
        AxonRequest::Status(_) => Route::get("/v1/status"),
        AxonRequest::Doctor(_) => Route::get("/v1/doctor"),
        AxonRequest::Sources(req) => Route::get(page_path(
            "/v1/sources",
            req.limit,
            req.offset,
            req.domain.as_deref(),
            req.cursor.as_deref(),
        )),
        AxonRequest::Domains(req) => Route::get(page_path(
            "/v1/domains",
            req.limit,
            req.offset,
            req.domain.as_deref(),
            None,
        )),
        AxonRequest::Stats(_) => Route::get("/v1/stats"),
        AxonRequest::Query(req) => Route::post(
            "/v1/query",
            rest_body_allowed(req, &["query", "collection", "limit", "offset"])?,
        ),
        AxonRequest::Retrieve(req) => Route::post(
            "/v1/retrieve",
            rest_body_allowed(req, &["url", "max_points", "cursor", "token_budget"])?,
        ),
        AxonRequest::Scrape(req) => {
            Route::post("/v1/scrape", rest_body_allowed(req, &["url", "embed"])?)
        }
        AxonRequest::Summarize(req) => {
            Route::post("/v1/summarize", rest_body_allowed(req, &["url", "urls"])?)
        }
        AxonRequest::Crawl(req) => async_route(
            "crawl",
            req.subaction.unwrap_or(CrawlSubaction::Start),
            req.job_id.as_deref(),
            req.limit,
            req.offset,
            rest_body_allowed(
                req,
                &[
                    "urls",
                    "max_pages",
                    "max_depth",
                    "render_mode",
                    "include_subdomains",
                    "respect_robots",
                    "discover_sitemaps",
                    "sitemap_since_days",
                    "discover_llms_txt",
                    "max_llms_txt_urls",
                    "delay_ms",
                ],
            )?,
        )?,
        AxonRequest::Extract(req) => async_route(
            "extract",
            req.subaction.unwrap_or(ExtractSubaction::Start),
            req.job_id.as_deref(),
            req.limit,
            req.offset,
            rest_body_allowed(
                req,
                &["urls", "prompt", "max_pages", "render_mode", "embed"],
            )?,
        )?,
        AxonRequest::Embed(req) => async_route(
            "embed",
            req.subaction.unwrap_or(EmbedSubaction::Start),
            req.job_id.as_deref(),
            req.limit,
            req.offset,
            rest_body_allowed(req, &["input"])?,
        )?,
        AxonRequest::Ingest(req) => async_route(
            "ingest",
            req.subaction.unwrap_or(IngestSubaction::Start),
            req.job_id.as_deref(),
            req.limit,
            req.offset,
            rest_body_allowed(
                req,
                &["source_type", "target", "include_source", "sessions"],
            )?,
        )?,
        _ => return Ok(None),
    };

    let value = match route.method {
        Method::Get => client
            .get_json(&route.path, "mcp thin client")
            .await
            .map_err(ThinClientError::Server)?,
        Method::Post => client
            .post_json(&route.path, &route.body, "mcp thin client")
            .await
            .map_err(ThinClientError::Server)?,
        Method::Delete => client
            .delete_json(&route.path, "mcp thin client")
            .await
            .map_err(ThinClientError::Server)?,
    };
    Ok(Some(value))
}

#[derive(Debug)]
struct Route {
    method: Method,
    path: String,
    body: Value,
}

#[derive(Debug)]
enum Method {
    Get,
    Post,
    Delete,
}

impl Route {
    fn get(path: impl Into<String>) -> Self {
        Self {
            method: Method::Get,
            path: path.into(),
            body: Value::Null,
        }
    }

    fn post(path: impl Into<String>, body: Value) -> Self {
        Self {
            method: Method::Post,
            path: path.into(),
            body,
        }
    }

    fn delete(path: impl Into<String>) -> Self {
        Self {
            method: Method::Delete,
            path: path.into(),
            body: Value::Null,
        }
    }
}

trait AsyncSubaction {
    fn route_name(&self) -> &'static str;
}

impl AsyncSubaction for CrawlSubaction {
    fn route_name(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Status => "status",
            Self::Cancel => "cancel",
            Self::List => "list",
            Self::Cleanup => "cleanup",
            Self::Clear => "clear",
            Self::Recover => "recover",
        }
    }
}

impl AsyncSubaction for ExtractSubaction {
    fn route_name(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Status => "status",
            Self::Cancel => "cancel",
            Self::List => "list",
            Self::Cleanup => "cleanup",
            Self::Clear => "clear",
            Self::Recover => "recover",
        }
    }
}

impl AsyncSubaction for EmbedSubaction {
    fn route_name(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Status => "status",
            Self::Cancel => "cancel",
            Self::List => "list",
            Self::Cleanup => "cleanup",
            Self::Clear => "clear",
            Self::Recover => "recover",
        }
    }
}

impl AsyncSubaction for IngestSubaction {
    fn route_name(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Status => "status",
            Self::Cancel => "cancel",
            Self::List => "list",
            Self::Cleanup => "cleanup",
            Self::Clear => "clear",
            Self::Recover => "recover",
        }
    }
}

fn async_route<S: AsyncSubaction>(
    family: &str,
    subaction: S,
    job_id: Option<&str>,
    list_limit: Option<i64>,
    list_offset: Option<usize>,
    body: Value,
) -> Result<Route, ThinClientError> {
    match subaction.route_name() {
        "start" => Ok(Route::post(format!("/v1/{family}"), body)),
        "list" => Ok(Route::get(page_path_i64(
            &format!("/v1/{family}"),
            list_limit,
            list_offset,
        )?)),
        "cleanup" => Ok(Route::post(format!("/v1/{family}/cleanup"), Value::Null)),
        "clear" => Ok(Route::delete(format!("/v1/{family}"))),
        "recover" => Ok(Route::post(format!("/v1/{family}/recover"), Value::Null)),
        "status" => Ok(Route::get(format!(
            "/v1/{family}/{}",
            required_job_id(family, "status", job_id)?
        ))),
        "cancel" => Ok(Route::post(
            format!(
                "/v1/{family}/{}/cancel",
                required_job_id(family, "cancel", job_id)?
            ),
            Value::Null,
        )),
        other => Err(ThinClientError::InvalidRequest(format!(
            "unsupported {family} subaction: {other}"
        ))),
    }
}

fn required_job_id<'a>(
    family: &str,
    subaction: &str,
    job_id: Option<&'a str>,
) -> Result<&'a str, ThinClientError> {
    job_id.filter(|id| !id.trim().is_empty()).ok_or_else(|| {
        ThinClientError::InvalidRequest(format!("{family} {subaction} requires job_id"))
    })
}

fn page_path(
    base: &str,
    limit: Option<usize>,
    offset: Option<usize>,
    domain: Option<&str>,
    cursor: Option<&str>,
) -> String {
    let mut pairs = Vec::new();
    if let Some(limit) = limit {
        pairs.push(("limit".to_string(), limit.to_string()));
    }
    if let Some(offset) = offset {
        pairs.push(("offset".to_string(), offset.to_string()));
    }
    if let Some(domain) = domain {
        pairs.push(("domain".to_string(), domain.to_string()));
    }
    if let Some(cursor) = cursor {
        pairs.push(("cursor".to_string(), cursor.to_string()));
    }
    if pairs.is_empty() {
        base.to_string()
    } else {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in pairs {
            serializer.append_pair(&key, &value);
        }
        format!("{base}?{}", serializer.finish())
    }
}

pub(crate) fn page_path_i64(
    base: &str,
    limit: Option<i64>,
    offset: Option<usize>,
) -> Result<String, ThinClientError> {
    let limit = limit
        .map(|value| {
            usize::try_from(value).map_err(|_| {
                ThinClientError::InvalidRequest("limit must be a non-negative integer".to_string())
            })
        })
        .transpose()?;
    Ok(page_path(base, limit, offset, None, None))
}

fn rest_body_allowed<T: serde::Serialize>(
    request: &T,
    allowed: &[&str],
) -> Result<Value, ThinClientError> {
    let mut value = serde_json::to_value(request)
        .map_err(|err| ThinClientError::InvalidRequest(format!("serialize MCP request: {err}")))?;
    let Some(object) = value.as_object_mut() else {
        return Ok(value);
    };
    object.retain(|key, _| allowed.contains(&key.as_str()));
    normalize_rest_enum_values(object);
    Ok(value)
}

fn normalize_rest_enum_values(object: &mut Map<String, Value>) {
    if let Some(Value::String(render_mode)) = object.get_mut("render_mode")
        && render_mode == "auto_switch"
    {
        *render_mode = "auto-switch".to_string();
    }
    if let Some(Value::String(format)) = object.get_mut("format")
        && format == "raw_html"
    {
        *format = "rawHtml".to_string();
    }
}
