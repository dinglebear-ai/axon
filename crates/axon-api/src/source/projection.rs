use axon_error::{ApiError, ErrorStage};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::*;

pub const PROJECTION_CONTRACT_VERSION: &str = "2026-08-23";
pub type ProjectionResult<T> = Result<T, Box<ApiError>>;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionOperation {
    Scrape,
    Crawl,
    Embed,
    Ingest,
    CodeSearch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceProjectionInput {
    pub input: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct QueryProjectionInput {
    pub input: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchRequest<I, P> {
    pub inputs: Vec<I>,
    pub options: P,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    Accepted,
    Completed,
    CompletedDegraded,
    Canceled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchSummary {
    pub total: usize,
    pub completed: usize,
    pub queued: usize,
    pub failed: usize,
    pub canceled: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchResult<T> {
    pub batch_id: BatchId,
    pub status: BatchStatus,
    pub items: Vec<BatchItem<T>>,
    pub summary: BatchSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchItem<T> {
    pub index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    pub outcome: BatchOutcome<T>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum BatchOutcome<T> {
    Completed(T),
    Queued(JobDescriptor),
    Failed(ApiError),
    Canceled,
}

macro_rules! source_projection_options {
    ($name:ident { $($extra:tt)* }) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub collection: Option<String>,
            #[serde(default)]
            pub refresh: SourceRefreshPolicy,
            #[serde(default)]
            pub execution: ExecutionPolicy,
            #[serde(default)]
            pub output: OutputPolicy,
            #[serde(default)]
            pub options: AdapterOptions,
            $($extra)*
        }
    };
}

source_projection_options!(ScrapeOptions {});
source_projection_options!(CrawlOptions {
    #[serde(default)]
    pub limits: SourceLimits,
});
source_projection_options!(EmbedOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<SourceScope>,
    #[serde(default)]
    pub limits: SourceLimits,
});
source_projection_options!(IngestOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<SourceScope>,
    #[serde(default)]
    pub limits: SourceLimits,
    #[serde(default)]
    pub no_embed: bool,
});

macro_rules! impl_source_options_default {
    ($name:ident { $($extra:ident : $value:expr),* $(,)? }) => {
        impl Default for $name {
            fn default() -> Self {
                Self {
                    collection: None,
                    refresh: SourceRefreshPolicy::default(),
                    execution: ExecutionPolicy::default(),
                    output: OutputPolicy::default(),
                    options: AdapterOptions::default(),
                    $($extra: $value,)*
                }
            }
        }
    };
}

impl_source_options_default!(ScrapeOptions {});
impl_source_options_default!(CrawlOptions {
    limits: SourceLimits::default()
});
impl_source_options_default!(EmbedOptions {
    scope: None,
    limits: SourceLimits::default(),
});
impl_source_options_default!(IngestOptions {
    scope: None,
    limits: SourceLimits::default(),
    no_embed: false,
});

macro_rules! source_projection_request {
    ($name:ident, $options:ident) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            pub inputs: Vec<SourceProjectionInput>,
            #[serde(default)]
            pub options: $options,
        }
    };
}

source_projection_request!(ScrapeRequest, ScrapeOptions);
source_projection_request!(CrawlRequest, CrawlOptions);
source_projection_request!(EmbedRequest, EmbedOptions);
source_projection_request!(IngestRequest, IngestOptions);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeSearchProjectionOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(default = "default_code_search_limit")]
    #[schemars(range(min = 1))]
    #[schema(minimum = 1)]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl Default for CodeSearchProjectionOptions {
    fn default() -> Self {
        Self {
            collection: None,
            limit: default_code_search_limit(),
            offset: 0,
            path_prefix: None,
            language: None,
            source: None,
        }
    }
}

const fn default_code_search_limit() -> usize {
    20
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeSearchRequest {
    pub inputs: Vec<QueryProjectionInput>,
    #[serde(default)]
    pub options: CodeSearchProjectionOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeSearchPlan {
    pub query: String,
    pub content_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    pub limit: usize,
    pub offset: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

pub fn project_scrape(request: &ScrapeRequest) -> ProjectionResult<Vec<SourceRequest>> {
    validate_source_inputs(&request.inputs)?;
    Ok(request
        .inputs
        .iter()
        .map(|input| {
            let mut source = base_source_request(input, &request.options);
            source.scope = Some(SourceScope::Page);
            source.limits.max_pages = Some(1);
            source.limits.max_items = Some(1);
            source
        })
        .collect())
}

pub fn project_crawl(request: &CrawlRequest) -> ProjectionResult<Vec<SourceRequest>> {
    validate_source_inputs(&request.inputs)?;
    Ok(request
        .inputs
        .iter()
        .map(|input| {
            let mut source = base_source_request(input, &request.options);
            source.scope = Some(SourceScope::Site);
            source.limits = request.options.limits.clone();
            source.embed = true;
            source
        })
        .collect())
}

pub fn project_embed(request: &EmbedRequest) -> ProjectionResult<Vec<SourceRequest>> {
    validate_source_inputs(&request.inputs)?;
    Ok(request
        .inputs
        .iter()
        .map(|input| {
            let mut source = base_source_request(input, &request.options);
            source.scope = request.options.scope;
            source.limits = request.options.limits.clone();
            source.embed = true;
            source
        })
        .collect())
}

pub fn project_ingest(request: &IngestRequest) -> ProjectionResult<Vec<SourceRequest>> {
    validate_source_inputs(&request.inputs)?;
    Ok(request
        .inputs
        .iter()
        .map(|input| {
            let mut source = base_source_request(input, &request.options);
            source.scope = request.options.scope;
            source.limits = request.options.limits.clone();
            source.embed = !request.options.no_embed;
            source
        })
        .collect())
}

pub fn project_code_search(request: &CodeSearchRequest) -> ProjectionResult<Vec<CodeSearchPlan>> {
    validate_query_inputs(&request.inputs)?;
    Ok(request
        .inputs
        .iter()
        .map(|input| CodeSearchPlan {
            query: input.input.clone(),
            content_kind: "code".to_string(),
            collection: request.options.collection.clone(),
            limit: request.options.limit,
            offset: request.options.offset,
            path_prefix: request.options.path_prefix.clone(),
            language: request.options.language.clone(),
            source: request.options.source.clone(),
        })
        .collect())
}

trait CommonSourceProjectionOptions {
    fn collection(&self) -> &Option<String>;
    fn refresh(&self) -> SourceRefreshPolicy;
    fn execution(&self) -> &ExecutionPolicy;
    fn output(&self) -> &OutputPolicy;
    fn adapter_options(&self) -> &AdapterOptions;
}

macro_rules! impl_common_source_options {
    ($($name:ident),+ $(,)?) => {$(
        impl CommonSourceProjectionOptions for $name {
            fn collection(&self) -> &Option<String> { &self.collection }
            fn refresh(&self) -> SourceRefreshPolicy { self.refresh }
            fn execution(&self) -> &ExecutionPolicy { &self.execution }
            fn output(&self) -> &OutputPolicy { &self.output }
            fn adapter_options(&self) -> &AdapterOptions { &self.options }
        }
    )+};
}

impl_common_source_options!(ScrapeOptions, CrawlOptions, EmbedOptions, IngestOptions);

fn base_source_request(
    input: &SourceProjectionInput,
    options: &impl CommonSourceProjectionOptions,
) -> SourceRequest {
    let mut request = SourceRequest::new(input.input.clone());
    request.collection.clone_from(options.collection());
    request.refresh = options.refresh();
    request.execution = options.execution().clone();
    request.output = options.output().clone();
    request.options = options.adapter_options().clone();
    request.idempotency_key.clone_from(&input.idempotency_key);
    request
}

fn validate_source_inputs(inputs: &[SourceProjectionInput]) -> ProjectionResult<()> {
    validate_inputs(inputs.iter().map(|input| input.input.as_str()))
}

fn validate_query_inputs(inputs: &[QueryProjectionInput]) -> ProjectionResult<()> {
    validate_inputs(inputs.iter().map(|input| input.input.as_str()))
}

fn validate_inputs<'a>(inputs: impl IntoIterator<Item = &'a str>) -> ProjectionResult<()> {
    let mut saw_input = false;
    for input in inputs {
        saw_input = true;
        if input.trim().is_empty() {
            return Err(Box::new(projection_error(
                "projection.input_empty",
                "projection inputs must not be empty",
            )));
        }
    }
    if !saw_input {
        return Err(Box::new(projection_error(
            "projection.inputs_empty",
            "at least one projection input is required",
        )));
    }
    Ok(())
}

pub(crate) fn projection_error(code: &str, message: &str) -> ApiError {
    ApiError::new(code, ErrorStage::Validation, message)
}
