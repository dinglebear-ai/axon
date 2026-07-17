use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

use axon_api::source::{
    BatchId, ChunkId, ContentKind, EmbeddingBatch, EmbeddingInput, JobId, JobPriority, MetadataMap,
    RedactionMetadata, SourceGenerationId, SourceId, SourceRange, VectorSearchRequest,
};
use axon_api::{CanonicalCitation, QueryHit};
use axon_core::config::Config;
use axon_embedding::reservation::ProviderReservationContext;
use axon_vectors::payload::generation_payload_i64;
use serde_json::json;

use crate::context::ServiceContext;
use crate::query::wrap_service_error;
use crate::types::{CodeSearchCaller, CodeSearchFreshness, CodeSearchOptions, CodeSearchResult};

pub use self::refresh::{
    CodeSearchProjectResult, CodeSearchRefreshResult, refresh_code_search_index,
    refresh_code_search_index_with_progress, resolve_code_search_project,
};
use self::support::{CodeIndexIdentity, CodeSearchAllowedRoots, validate_path_prefix};
pub use self::support::{FreshnessWarning, ReindexProgress, ReindexProgressSink};

#[path = "code_search_refresh.rs"]
mod refresh;
#[path = "code_search_support.rs"]
mod support;

const MAX_CODE_SEARCH_QUERY_LEN_BYTES: usize = 64 * 1024;
const CODE_SEARCH_GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Search one local git checkout after optionally refreshing its local-code vectors.
#[must_use = "code_search returns a Result that should be handled"]
pub async fn code_search(
    ctx: &ServiceContext,
    text: &str,
    opts: CodeSearchOptions,
) -> Result<CodeSearchResult, Box<dyn Error + Send + Sync>> {
    code_search_with_progress(ctx, text, opts, None).await
}

#[must_use = "code_search_with_progress returns a Result that should be handled"]
pub async fn code_search_with_progress(
    ctx: &ServiceContext,
    text: &str,
    opts: CodeSearchOptions,
    progress: Option<&dyn ReindexProgressSink>,
) -> Result<CodeSearchResult, Box<dyn Error + Send + Sync>> {
    if text.len() > MAX_CODE_SEARCH_QUERY_LEN_BYTES {
        return Err(format!(
            "code_search query exceeds {MAX_CODE_SEARCH_QUERY_LEN_BYTES}-byte cap (got {} bytes)",
            text.len()
        )
        .into());
    }

    let path_prefix = opts
        .path_prefix
        .as_deref()
        .map(validate_path_prefix)
        .transpose()?
        .flatten();
    target_code_search(ctx, text, opts, path_prefix.as_deref(), progress).await
}

async fn target_code_search(
    ctx: &ServiceContext,
    text: &str,
    opts: CodeSearchOptions,
    path_prefix: Option<&str>,
    progress: Option<&dyn ReindexProgressSink>,
) -> Result<CodeSearchResult, Box<dyn Error + Send + Sync>> {
    let refresh = if opts.ensure_fresh {
        refresh_code_search_index_with_progress(ctx, opts.cwd.as_deref(), opts.caller, progress)
            .await?
    } else {
        refresh::target_code_search_committed_state(ctx, opts.cwd.as_deref(), opts.caller).await?
    };
    let Some(source_id) = refresh.target_source_id.clone() else {
        return Ok(code_search_missing_index_result(text, refresh.freshness));
    };
    let Some(generation) = refresh.target_source_generation.clone() else {
        return Ok(code_search_missing_index_result(text, refresh.freshness));
    };
    let Some(target) = ctx.target_local_source_runtime() else {
        return Ok(code_search_missing_index_result(text, refresh.freshness));
    };
    let _reservation = target
        .embedding_reservations
        .reserve_with_context(ProviderReservationContext {
            job_id: JobId::new(uuid::Uuid::new_v4()),
            stage_id: None,
            provider_id: Some(target.embedding_provider_id.clone()),
            priority: JobPriority::Interactive,
            units: 1,
            ttl_seconds: Some(60),
        })
        .await
        .map_err(|e| -> Box<dyn Error + Send + Sync> {
            wrap_service_error(
                "target code_search query embedding reservation failed".to_string(),
                &e,
            )
        })?;
    let embedding = target
        .embedding_provider
        .embed(EmbeddingBatch {
            batch_id: BatchId::new(uuid::Uuid::new_v4()),
            job_id: JobId::new(uuid::Uuid::new_v4()),
            provider_id: target.embedding_provider_id.clone(),
            model: target.embedding_model.clone(),
            items: vec![EmbeddingInput {
                chunk_id: ChunkId::new("code-search-query"),
                text: text.to_string(),
                content_kind: ContentKind::Code,
                metadata: MetadataMap::new(),
            }],
            instruction: None,
            priority: JobPriority::Interactive,
            metadata: MetadataMap::new(),
        })
        .await
        .map_err(|e| -> Box<dyn Error + Send + Sync> {
            wrap_service_error("target code_search query embedding failed".to_string(), &e)
        })?;
    let dense_vector = embedding
        .vectors
        .first()
        .map(|vector| vector.values.clone())
        .ok_or("target code_search query embedding returned no vector")?;

    let request = target_code_search_request(
        ctx.cfg().collection.clone(),
        text,
        opts.limit.saturating_add(opts.offset).max(1),
        dense_vector,
        &source_id,
        &generation,
        path_prefix,
    )?;
    let matches =
        target
            .vector_store
            .search(request)
            .await
            .map_err(|e| -> Box<dyn Error + Send + Sync> {
                wrap_service_error("target code_search vector query failed".to_string(), &e)
            })?;
    let results = matches
        .results
        .into_iter()
        .skip(opts.offset)
        .take(opts.limit.clamp(1, axon_api::MAX_CANONICAL_CITATIONS))
        .enumerate()
        .map(|(index, hit)| target_vector_match_to_query_hit(index as u64 + 1, hit))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(CodeSearchResult {
        query: text.to_string(),
        content_trust: "untrusted_local_code".to_string(),
        results,
        freshness: refresh.freshness,
    })
}

fn target_code_search_request(
    collection: String,
    query: &str,
    limit: usize,
    dense_vector: Vec<f32>,
    source_id: &SourceId,
    committed_generation: &SourceGenerationId,
    path_prefix: Option<&str>,
) -> Result<VectorSearchRequest, Box<dyn Error + Send + Sync>> {
    let mut filters = MetadataMap::new();
    filters.insert("source_id".to_string(), json!(source_id.0));
    filters.insert(
        "committed_generation".to_string(),
        json!(generation_payload_i64(
            committed_generation,
            "committed_generation"
        )?),
    );
    filters.insert("visibility".to_string(), json!("public"));
    filters.insert("redaction_status".to_string(), json!("clean"));
    if let Some(prefix) = path_prefix {
        filters.insert("path_prefix".to_string(), json!(prefix));
    }
    Ok(VectorSearchRequest {
        collection,
        query: query.to_string(),
        limit: u32::try_from(limit).unwrap_or(u32::MAX),
        dense_vector: Some(dense_vector),
        sparse_vector: None,
        filters,
        hybrid: Some(false),
        generation: None,
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    })
}

fn target_vector_match_to_query_hit(
    rank: u64,
    hit: axon_api::source::VectorSearchMatch,
) -> Result<QueryHit, Box<dyn Error + Send + Sync>> {
    let source_range = hit.payload.get("source_range");
    let chunk_locator = hit.payload.get("chunk_locator");
    let canonical_uri = payload_string(&hit.payload, "item_canonical_uri")
        .or_else(|| payload_string(&hit.payload, "source_item_key"))
        .unwrap_or_else(|| hit.point_id.0.clone());
    let citation = code_search_citation(&hit, &canonical_uri)?;
    Ok(QueryHit {
        rank,
        score: hit.score,
        rerank_score: hit.score,
        url: canonical_uri,
        source: payload_string(&hit.payload, "source_item_key").unwrap_or_default(),
        snippet: hit.text.unwrap_or_default(),
        citation,
        chunk_index: None,
        file_path: chunk_locator_path(chunk_locator)
            .or_else(|| payload_string(&hit.payload, "source_item_key")),
        symbol: chunk_locator
            .and_then(|value| value.get("symbol"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        kind: None,
        start_line: source_range_u32(source_range, "line_start"),
        end_line: source_range_u32(source_range, "line_end"),
        file_type: None,
        language: None,
        provider: payload_string(&hit.payload, "embedding_provider"),
        content_kind: payload_string(&hit.payload, "content_kind"),
        chunking_method: None,
        symbol_extraction_status: None,
    })
}

fn code_search_citation(
    hit: &axon_api::source::VectorSearchMatch,
    canonical_uri: &str,
) -> Result<CanonicalCitation, Box<dyn Error + Send + Sync>> {
    let source_id = hit
        .source_id
        .clone()
        .ok_or_else(|| missing_lineage("source_id"))?;
    let source_item_key = hit
        .source_item_key
        .clone()
        .ok_or_else(|| missing_lineage("source_item_key"))?;
    let document_id = hit
        .document_id
        .clone()
        .ok_or_else(|| missing_lineage("document_id"))?;
    let chunk_id = hit
        .chunk_id
        .clone()
        .ok_or_else(|| missing_lineage("chunk_id"))?;
    let generation = hit
        .payload
        .get("source_generation")
        .and_then(|value| {
            value
                .as_str()
                .map(ToString::to_string)
                .or_else(|| value.as_i64().map(|value| value.to_string()))
        })
        .map(SourceGenerationId::new)
        .ok_or_else(|| missing_lineage("source_generation"))?;
    let job_id = payload_string(&hit.payload, "job_id")
        .and_then(|value| uuid::Uuid::parse_str(&value).ok())
        .map(JobId::new)
        .ok_or_else(|| missing_lineage("job_id"))?;
    let source_range = hit
        .payload
        .get("source_range")
        .cloned()
        .map(serde_json::from_value::<SourceRange>)
        .transpose()?
        .ok_or_else(|| missing_lineage("source_range"))?;
    let redaction = RedactionMetadata {
        redaction_status: payload_value(&hit.payload, "redaction_status")?,
        redaction_version: required_payload_string(&hit.payload, "redaction_version")?,
        visibility: payload_value(&hit.payload, "visibility")?,
        redacted_field_count: required_payload_u32(&hit.payload, "redacted_field_count")?,
        dropped_field_count: required_payload_u32(&hit.payload, "dropped_field_count")?,
        detector_count: required_payload_u32(&hit.payload, "detector_count")?,
        detector_names: payload_value(&hit.payload, "detector_names")?,
    };
    Ok(CanonicalCitation {
        source_id,
        source_item_key,
        generation,
        document_id,
        chunk_id,
        job_id,
        canonical_uri: canonical_uri.to_string(),
        source_range,
        redaction,
    })
}

fn payload_value<T: serde::de::DeserializeOwned>(
    payload: &MetadataMap,
    field: &str,
) -> Result<T, Box<dyn Error + Send + Sync>> {
    payload
        .get(field)
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .ok_or_else(|| missing_lineage(field))
}

fn required_payload_string(
    payload: &MetadataMap,
    field: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    payload_string(payload, field).ok_or_else(|| missing_lineage(field))
}

fn required_payload_u32(
    payload: &MetadataMap,
    field: &str,
) -> Result<u32, Box<dyn Error + Send + Sync>> {
    payload
        .get(field)
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| missing_lineage(field))
}

fn missing_lineage(field: &str) -> Box<dyn Error + Send + Sync> {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("code search hit is missing canonical lineage field `{field}`"),
    )
    .into()
}

fn payload_string(payload: &MetadataMap, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn chunk_locator_path(locator: Option<&serde_json::Value>) -> Option<String> {
    locator?
        .get("path")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn source_range_u32(range: Option<&serde_json::Value>, field: &str) -> Option<u32> {
    range?
        .get(field)
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
}

fn code_search_missing_index_result(
    text: &str,
    freshness: CodeSearchFreshness,
) -> CodeSearchResult {
    CodeSearchResult {
        query: text.to_string(),
        content_trust: "untrusted_local_code".to_string(),
        results: Vec::new(),
        freshness: code_search_missing_index_freshness(freshness),
    }
}

pub(crate) fn code_search_freshness(
    status: &str,
    warning: Option<FreshnessWarning>,
    indexed_files: usize,
    removed_files: usize,
) -> CodeSearchFreshness {
    let status = if warning.is_some() { "stale" } else { status };
    CodeSearchFreshness {
        status: status.to_string(),
        warning: warning.map(|warning| warning.message()),
        indexed_files,
        removed_files,
    }
}

pub(crate) fn code_search_missing_index_freshness(
    mut freshness: CodeSearchFreshness,
) -> CodeSearchFreshness {
    if freshness.warning.is_none() {
        freshness.status = "stale".to_string();
        freshness.warning = Some(FreshnessWarning::MissingCommittedIndex.message());
    }
    freshness
}

pub(crate) async fn resolve_code_search_root(
    cwd: Option<&Path>,
    caller: CodeSearchCaller,
) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let cwd = match (caller, cwd) {
        (CodeSearchCaller::Cli, Some(cwd)) => cwd.to_path_buf(),
        (CodeSearchCaller::Cli, None) => std::env::current_dir()?,
        (CodeSearchCaller::Mcp, Some(cwd)) => cwd.to_path_buf(),
        (CodeSearchCaller::Mcp, None) => {
            return Err("code_search MCP requests must provide cwd".into());
        }
    };
    let canonical_cwd =
        std::fs::canonicalize(&cwd).map_err(|_| "code_search cwd could not be resolved")?;
    let git_root = git_toplevel(&canonical_cwd).await?;
    reject_unsafe_code_root(&git_root)?;
    if matches!(caller, CodeSearchCaller::Mcp) {
        let allowed = CodeSearchAllowedRoots::from_env()?;
        if !allowed.contains(&git_root) {
            return Err(code_search_outside_allowed_roots_message().into());
        }
    }
    Ok(git_root)
}

pub(crate) fn code_search_outside_allowed_roots_message() -> &'static str {
    "code_search cwd is outside AXON_CODE_SEARCH_ALLOWED_ROOTS"
}

async fn git_toplevel(cwd: &Path) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let cwd = cwd.to_path_buf();
    let output = tokio::time::timeout(
        CODE_SEARCH_GIT_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            Command::new("git")
                .arg("-C")
                .arg(cwd)
                .args(["rev-parse", "--show-toplevel"])
                .output()
        }),
    )
    .await
    .map_err(|_| "git rev-parse timed out")?
    .map_err(|e| format!("git rev-parse task failed: {e}"))?
    .map_err(|_| "code_search cwd is not inside a git checkout")?;
    if !output.status.success() {
        return Err("code_search cwd is not inside a git checkout".into());
    }
    let root = String::from_utf8(output.stdout)
        .map_err(|e| format!("git rev-parse output was not UTF-8: {e}"))?;
    let root = root.trim();
    if root.is_empty() {
        return Err("git rev-parse returned an empty repository root".into());
    }
    std::fs::canonicalize(root).map_err(Into::into)
}

fn reject_unsafe_code_root(root: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
    if root == Path::new("/") {
        return Err("code_search refuses to index filesystem root".into());
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && root == home.as_path()
    {
        return Err("code_search refuses to index HOME directly".into());
    }
    Ok(())
}

pub(super) async fn code_search_identity(cfg: &Config, project_root: PathBuf) -> CodeIndexIdentity {
    let origin = code_search_project_origin(&project_root).await;
    let embedder = if cfg.tei_url.trim().is_empty() {
        "tei".to_string()
    } else {
        cfg.tei_url.clone()
    };
    CodeIndexIdentity::new(project_root, origin, &cfg.collection, &embedder)
}

pub(crate) async fn code_search_project_origin(project_root: &Path) -> String {
    let remote = match git_remote_origin(project_root).await {
        Ok(Some(remote)) => remote,
        Ok(None) => "git:no-origin".to_string(),
        Err(error) => {
            tracing::warn!(
                %error,
                project_root = %project_root.display(),
                "code_search git remote origin lookup failed; using checkout-scoped fallback"
            );
            "git:no-origin".to_string()
        }
    };
    // This seed is private input to the UUID project key. Only the derived key is
    // stored in Qdrant payloads; the absolute root remains SQLite-only.
    format!("{remote}\nworktree:{}", project_root.display())
}

async fn git_remote_origin(project_root: &Path) -> Result<Option<String>, String> {
    let project_root = project_root.to_path_buf();
    let output = tokio::time::timeout(
        CODE_SEARCH_GIT_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            Command::new("git")
                .arg("-C")
                .arg(project_root)
                .args(["config", "--get", "remote.origin.url"])
                .output()
        }),
    )
    .await
    .map_err(|_| "git remote origin lookup timed out".to_string())?
    .map_err(|err| format!("git remote origin lookup task failed: {err}"))?
    .map_err(|err| format!("git remote origin lookup failed to spawn git: {err}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let origin = String::from_utf8(output.stdout)
        .map_err(|err| format!("git remote origin output was not UTF-8: {err}"))?;
    let origin = origin.trim();
    Ok((!origin.is_empty()).then(|| format!("git:{origin}")))
}

#[cfg(test)]
#[path = "code_search_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "code_search_target_tests.rs"]
mod target_tests;
