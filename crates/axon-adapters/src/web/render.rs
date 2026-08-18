//! Render-backed web acquisition helpers.

use axon_api::source::*;
use axon_core::logging::log_warn;

use crate::adapter::Result;
use crate::boundary::RenderProvider;

use super::acquire::AcquiredItem;
use super::binary::reject_binary_rendered_payload;

pub(super) async fn acquire_via_auto_switch(
    render: &dyn RenderProvider,
    item: &ManifestItem,
    min_markdown_chars: usize,
    automation_script: Option<ArtifactRef>,
    render_metadata: MetadataMap,
    mut warnings: Vec<SourceWarning>,
) -> Result<AcquiredItem> {
    let first = render
        .render(build_render_request(
            item,
            RenderMode::Http,
            automation_script.clone(),
            render_metadata.clone(),
        ))
        .await?;
    if first.markdown.chars().count() >= min_markdown_chars {
        return Ok(AcquiredItem {
            item: Some(acquired_from_rendered(item, first, "auto_switch_http")?),
            warnings,
        });
    }
    match render
        .render(build_render_request(
            item,
            RenderMode::Chrome,
            automation_script,
            render_metadata,
        ))
        .await
    {
        Ok(rendered) => Ok(AcquiredItem {
            item: Some(acquired_from_rendered(
                item,
                rendered,
                "auto_switch_chrome",
            )?),
            warnings,
        }),
        Err(err) => {
            log_warn(&format!(
                "auto_switch: chrome re-render failed for {} — keeping HTTP result: {err}",
                item.canonical_uri
            ));
            warnings.push(SourceWarning {
                code: "web.auto_switch.chrome_fallback_failed".to_string(),
                severity: Severity::Warning,
                message: format!(
                    "chrome re-render failed for {} — kept HTTP result: {err}",
                    item.canonical_uri
                ),
                source_item_key: Some(item.source_item_key.clone()),
                retryable: err.retryable,
            });
            Ok(AcquiredItem {
                item: Some(acquired_from_rendered(
                    item,
                    first,
                    "auto_switch_http_fallback",
                )?),
                warnings,
            })
        }
    }
}

pub(super) fn build_render_request(
    item: &ManifestItem,
    mode: RenderMode,
    automation_script: Option<ArtifactRef>,
    metadata: MetadataMap,
) -> RenderRequest {
    RenderRequest {
        uri: item.canonical_uri.clone(),
        mode,
        timeout_ms: None,
        wait_ms: None,
        automation_script,
        credential_refs: Vec::new(),
        metadata,
    }
}

pub(super) fn acquired_from_rendered(
    item: &ManifestItem,
    rendered: RenderedResource,
    method_tag: &'static str,
) -> Result<AcquiredSourceItem> {
    reject_binary_rendered_payload(item, &rendered.markdown)?;
    let mut manifest_item = item.clone();
    manifest_item.content_kind = Some(ContentKind::Markdown);
    manifest_item.content_hash = Some(super::manifest_items::content_ref_hash(
        &ContentRef::InlineText {
            text: rendered.markdown.clone(),
        },
    ));
    manifest_item.version = None;

    let mut metadata = MetadataMap::new();
    metadata.insert(
        "web_fetch_method".to_string(),
        serde_json::json!(method_tag),
    );
    metadata.insert(
        "web_render_mode".to_string(),
        serde_json::json!(render_mode_tag(rendered.render_mode)),
    );

    Ok(AcquiredSourceItem {
        manifest_item,
        fetch_status: LifecycleStatus::Completed,
        content_ref: ContentRef::InlineText {
            text: rendered.markdown,
        },
        raw_artifact_id: None,
        headers: RedactedHeaders {
            headers: Vec::new(),
        },
        fetched_at: rendered.captured_at,
        metadata,
    })
}

fn render_mode_tag(mode: RenderMode) -> &'static str {
    match mode {
        RenderMode::Http => "http",
        RenderMode::Chrome => "chrome",
        RenderMode::AutoSwitch => "auto_switch",
    }
}
