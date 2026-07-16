use super::super::AxonMcpServer;
use super::super::artifacts::{InlineHint, respond_with_mode};
use super::super::common::{invalid_params, logged_internal_error};
use super::super::system_requests::{UploadsMcpRequest, UploadsSubaction};
use crate::schema::AxonToolResponse;
use axon_api::source::{
    ContentRef, MetadataMap, UploadAbortRequest, UploadCompleteRequest, UploadCreateRequest,
    UploadId, UploadListRequest,
};
use base64::Engine as _;
use rmcp::ErrorData;
use serde_json::Value;

impl AxonMcpServer {
    pub(in crate::server) async fn handle_uploads(
        &self,
        req: UploadsMcpRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let subaction = req.subaction.unwrap_or_default();
        let ctx = self
            .base_service_context()
            .await
            .map_err(|error| logged_internal_error("uploads.context", error.as_ref()))?;
        let payload = match subaction {
            UploadsSubaction::List => serde_json::to_value(
                axon_services::uploads::list_uploads(
                    &ctx,
                    UploadListRequest {
                        status: req.status,
                        limit: req.limit,
                        cursor: req.cursor,
                    },
                )
                .await
                .map_err(|error| invalid_params(error.to_string()))?,
            ),
            UploadsSubaction::Create => serde_json::to_value(
                axon_services::uploads::create_upload(
                    &ctx,
                    UploadCreateRequest {
                        filename: required_upload_field(req.filename, "filename")?,
                        content_type: required_upload_field(req.content_type, "content_type")?,
                        size_bytes: req
                            .size_bytes
                            .ok_or_else(|| invalid_params("uploads create requires size_bytes"))?,
                        purpose: req
                            .purpose
                            .ok_or_else(|| invalid_params("uploads create requires purpose"))?,
                        sha256: req.sha256,
                        source_hint: req.source_hint,
                        source_id: None,
                        metadata: MetadataMap::new(),
                    },
                )
                .await
                .map_err(|error| invalid_params(error.to_string()))?,
            ),
            UploadsSubaction::Get => serde_json::to_value(
                axon_services::uploads::get_upload(&ctx, required_upload_id(req.upload_id)?)
                    .await
                    .map_err(|error| invalid_params(error.to_string()))?,
            ),
            UploadsSubaction::PutContent => {
                let bytes = upload_content_bytes(&ctx, req.content, req.content_ref).await?;
                serde_json::to_value(
                    axon_services::uploads::put_upload_content(
                        &ctx,
                        required_upload_id(req.upload_id)?,
                        bytes,
                        None,
                        req.sha256,
                    )
                    .await
                    .map_err(|error| invalid_params(error.to_string()))?,
                )
            }
            UploadsSubaction::Complete => serde_json::to_value(
                axon_services::uploads::complete_upload(
                    &ctx,
                    required_upload_id(req.upload_id)?,
                    UploadCompleteRequest {
                        sha256: req.sha256,
                        source_options: req.source_options.unwrap_or_default(),
                    },
                )
                .await
                .map_err(|error| invalid_params(error.to_string()))?,
            ),
            UploadsSubaction::Abort => serde_json::to_value(
                axon_services::uploads::abort_upload(
                    &ctx,
                    required_upload_id(req.upload_id)?,
                    UploadAbortRequest { reason: req.reason },
                )
                .await
                .map_err(|error| invalid_params(error.to_string()))?,
            ),
        }
        .unwrap_or(Value::Null);
        let label = match subaction {
            UploadsSubaction::List => "list",
            UploadsSubaction::Create => "create",
            UploadsSubaction::Get => "get",
            UploadsSubaction::PutContent => "put_content",
            UploadsSubaction::Complete => "complete",
            UploadsSubaction::Abort => "abort",
        };
        respond_with_mode(
            "uploads",
            label,
            req.response_mode,
            "uploads",
            payload,
            InlineHint::Default,
        )
        .await
    }
}

async fn upload_content_bytes(
    ctx: &axon_services::context::ServiceContext,
    content: Option<String>,
    content_ref: Option<ContentRef>,
) -> Result<Vec<u8>, ErrorData> {
    match (content, content_ref) {
        (Some(content), None) => base64::engine::general_purpose::STANDARD
            .decode(content)
            .map_err(|_| invalid_params("uploads put_content requires valid base64 content")),
        (None, Some(content_ref @ ContentRef::InlineText { .. }))
        | (None, Some(content_ref @ ContentRef::InlineBytes { .. })) => {
            inline_content_bytes(content_ref)
        }
        (None, Some(ContentRef::Artifact { artifact_id })) => {
            let artifact = axon_services::uploads::resolve_upload_artifact(ctx, &artifact_id.0)
                .await
                .map_err(|error| invalid_params(error.to_string()))?
                .ok_or_else(|| invalid_params("uploads content_ref artifact not found"))?;
            artifact
                .content
                .ok_or_else(|| invalid_params("uploads content_ref artifact has no bytes"))
                .and_then(inline_content_bytes)
        }
        (None, Some(ContentRef::External { .. })) => Err(invalid_params(
            "uploads content_ref must not fetch an external URI",
        )),
        (Some(_), Some(_)) => Err(invalid_params(
            "uploads put_content accepts exactly one of content or content_ref",
        )),
        (None, None) => Err(invalid_params(
            "uploads put_content requires content or content_ref",
        )),
    }
}

fn inline_content_bytes(content: ContentRef) -> Result<Vec<u8>, ErrorData> {
    match content {
        ContentRef::InlineText { text } => Ok(text.into_bytes()),
        ContentRef::InlineBytes { bytes_base64, .. } => base64::engine::general_purpose::STANDARD
            .decode(bytes_base64)
            .map_err(|_| invalid_params("uploads content_ref contains invalid base64")),
        ContentRef::Artifact { .. } | ContentRef::External { .. } => Err(invalid_params(
            "uploads resolved content_ref did not contain inline bytes",
        )),
    }
}

fn required_upload_id(value: Option<String>) -> Result<UploadId, ErrorData> {
    required_upload_field(value, "upload_id").map(UploadId::new)
}

fn required_upload_field(value: Option<String>, field: &str) -> Result<String, ErrorData> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_params(format!("uploads requires {field}")))
}
