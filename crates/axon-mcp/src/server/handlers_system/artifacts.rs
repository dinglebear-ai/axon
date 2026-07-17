use super::super::AxonMcpServer;
use super::super::artifacts::{InlineHint, respond_with_mode};
use super::super::common::{invalid_params, logged_internal_error};
use super::super::system_requests::{ArtifactsMcpRequest, ArtifactsSubaction};
use crate::schema::AxonToolResponse;
use axon_api::source::{ApiError, ArtifactId, ArtifactListRequest, JobId, SourceId};
use base64::Engine;
use rmcp::ErrorData;
use uuid::Uuid;

const MAX_INLINE_CONTENT_BYTES: u64 = 1024 * 1024;

/// Map a service artifact error onto the MCP error taxonomy. Identity errors
/// the caller can fix stay invalid-params; store/read failures (whose messages
/// may embed server filesystem paths) are logged server-side and returned as
/// redacted internal errors instead.
fn artifact_service_error(context: &'static str, error: ApiError) -> ErrorData {
    match error.code.0.as_str() {
        "artifact.not_found" | "artifact.invalid_id" => invalid_params(error.message),
        _ => logged_internal_error(context, &error),
    }
}

impl AxonMcpServer {
    pub(in crate::server) async fn handle_artifacts(
        &self,
        req: ArtifactsMcpRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let subaction = req.subaction.unwrap_or_default();
        let context = self
            .base_service_context()
            .await
            .map_err(|error| logged_internal_error("artifacts.context", error.as_ref()))?;
        let payload = match subaction {
            ArtifactsSubaction::List => serde_json::to_value(
                axon_services::artifacts::list_artifacts(
                    &context,
                    ArtifactListRequest {
                        source_id: req.source_id.map(SourceId::new),
                        job_id: req
                            .job_id
                            .map(|id| {
                                Uuid::parse_str(&id)
                                    .map(JobId::new)
                                    .map_err(|_| invalid_params("job_id must be a UUID"))
                            })
                            .transpose()?,
                        kind: req.kind,
                        limit: req.limit,
                        cursor: req.cursor,
                    },
                )
                .await
                .map_err(|error| artifact_service_error("artifacts.list", error))?,
            )
            .map_err(|error| logged_internal_error("artifacts.list", &error))?,
            ArtifactsSubaction::Get => serde_json::to_value(
                axon_services::artifacts::get_artifact(
                    &context,
                    ArtifactId::new(required_id(&req)?),
                )
                .await
                .map_err(|error| artifact_service_error("artifacts.get", error))?,
            )
            .map_err(|error| logged_internal_error("artifacts.get", &error))?,
            ArtifactsSubaction::Content => {
                let content = axon_services::artifacts::artifact_content(
                    &context,
                    ArtifactId::new(required_id(&req)?),
                )
                .await
                .map_err(|error| artifact_service_error("artifacts.content", error))?;
                if content.size_bytes > MAX_INLINE_CONTENT_BYTES {
                    return Err(invalid_params(format!(
                        "artifact content is {} bytes; MCP inline content is capped at {} bytes; use /v1/artifacts/{}/content",
                        content.size_bytes, MAX_INLINE_CONTENT_BYTES, content.artifact_id.0
                    )));
                }
                let bytes = tokio::fs::read(&content.path)
                    .await
                    .map_err(|error| logged_internal_error("artifacts.content", &error))?;
                let (encoding, body) = match std::str::from_utf8(&bytes) {
                    Ok(text) => ("utf8", text.to_string()),
                    Err(_) => (
                        "base64",
                        base64::engine::general_purpose::STANDARD.encode(bytes),
                    ),
                };
                serde_json::json!({
                    "artifact_id": content.artifact_id,
                    "content_type": content.content_type,
                    "size_bytes": content.size_bytes,
                    "encoding": encoding,
                    "content": body,
                })
            }
        };
        let label = match subaction {
            ArtifactsSubaction::List => "list",
            ArtifactsSubaction::Get => "get",
            ArtifactsSubaction::Content => "content",
        };
        respond_with_mode(
            "artifacts",
            label,
            req.response_mode,
            "artifacts",
            payload,
            InlineHint::Default,
        )
        .await
    }
}

fn required_id(req: &ArtifactsMcpRequest) -> Result<String, ErrorData> {
    req.artifact_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| invalid_params("artifacts get/content requires artifact_id"))
}

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;
