use super::{SessionWatchOptions, redact_error_detail};
use crate::sessions::checkpoint::SessionFileMetadata;
use crate::sessions::watch::validate::ValidatedSessionPath;
use anyhow::{Result, anyhow};
use sha2::Digest;
use std::path::Path;

pub(super) const MAX_UPLOAD_BODY_BYTES: usize = 25 * 1024 * 1024;
pub(super) const TARGET_UPLOAD_BODY_BYTES: usize = 24 * 1024 * 1024;

pub(super) type PreparedWatchDoc = (
    usize,
    ValidatedSessionPath,
    SessionFileMetadata,
    crate::sessions::PreparedSessionDoc,
    Option<String>,
);

pub(super) fn watch_upload_chunks<'a>(
    prepared_meta: &'a [PreparedWatchDoc],
    options: &SessionWatchOptions,
    project: Option<&String>,
    collection: Option<&String>,
) -> Result<Vec<Vec<&'a PreparedWatchDoc>>> {
    watch_upload_chunks_with_target(
        prepared_meta,
        options,
        project,
        collection,
        TARGET_UPLOAD_BODY_BYTES,
    )
}

fn watch_upload_chunks_with_target<'a>(
    prepared_meta: &'a [PreparedWatchDoc],
    options: &SessionWatchOptions,
    project: Option<&String>,
    collection: Option<&String>,
    target_upload_body_bytes: usize,
) -> Result<Vec<Vec<&'a PreparedWatchDoc>>> {
    if !options.upload_to_server {
        return Ok(prepared_meta
            .chunks(options.max_batch_docs.max(1))
            .map(|chunk| chunk.iter().collect())
            .collect());
    }

    let mut chunks: Vec<Vec<&PreparedWatchDoc>> = Vec::new();
    let mut current: Vec<&PreparedWatchDoc> = Vec::new();
    let mut current_doc_lens = Vec::new();
    for item in prepared_meta {
        let item_len = remote_upload_doc_body_len(item)?;
        let would_exceed_count = current.len() >= options.max_batch_docs.max(1);
        let would_exceed_bytes = !current.is_empty()
            && remote_upload_body_len_from_doc_lens(
                current_doc_lens
                    .iter()
                    .copied()
                    .chain(std::iter::once(item_len)),
                project,
                collection,
            )? > target_upload_body_bytes;
        if would_exceed_count || would_exceed_bytes {
            chunks.push(std::mem::take(&mut current));
            current_doc_lens.clear();
        }
        current.push(item);
        current_doc_lens.push(item_len);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    for chunk in &chunks {
        let len = remote_upload_body_len(chunk, project, collection)?;
        if len > target_upload_body_bytes {
            return Err(anyhow!(
                "prepared session upload chunk exceeds size limit: {len} bytes > {target_upload_body_bytes} bytes"
            ));
        }
    }
    Ok(chunks)
}

pub(super) fn remote_upload_body_len(
    items: &[&PreparedWatchDoc],
    project: Option<&String>,
    collection: Option<&String>,
) -> Result<usize> {
    remote_upload_body_len_from_doc_lens(
        items
            .iter()
            .map(|item| remote_upload_doc_body_len(item))
            .collect::<Result<Vec<_>>>()?,
        project,
        collection,
    )
}

fn remote_upload_doc_body_len(item: &PreparedWatchDoc) -> Result<usize> {
    Ok(serde_json::to_vec(&redact_remote_prepared_doc(item.3.clone()))?.len())
}

fn remote_upload_body_len_from_doc_lens(
    doc_lens: impl IntoIterator<Item = usize>,
    project: Option<&String>,
    collection: Option<&String>,
) -> Result<usize> {
    let doc_lens = doc_lens.into_iter().collect::<Vec<_>>();
    let docs_len = doc_lens.iter().sum::<usize>() + doc_lens.len().saturating_sub(1);
    Ok("{\"docs\":[".len()
        + docs_len
        + "],\"project\":".len()
        + serde_json::to_vec(&project)?.len()
        + ",\"collection\":".len()
        + serde_json::to_vec(&collection)?.len()
        + "}".len())
}

pub(super) async fn upload_prepared_sessions_to_server(
    request: crate::sessions::IngestSessionsPreparedRequest,
    options: &SessionWatchOptions,
) -> Result<String> {
    let base = options
        .upload_server_url
        .clone()
        .map(Ok)
        .unwrap_or_else(|| std::env::var("AXON_SERVER_URL"))
        .map_err(|_| anyhow!("AXON_SERVER_URL is required when --upload-to-server is set"))?;
    let token = options
        .upload_token
        .clone()
        .map(Ok)
        .unwrap_or_else(|| std::env::var("AXON_MCP_HTTP_TOKEN"))
        .map_err(|_| anyhow!("AXON_MCP_HTTP_TOKEN is required when --upload-to-server is set"))?;
    upload_prepared_sessions_to_server_with_auth(&base, &token, request).await
}

#[cfg_attr(not(test), allow(dead_code))]
pub async fn upload_prepared_sessions_to_server_with_auth(
    base: &str,
    token: &str,
    request: crate::sessions::IngestSessionsPreparedRequest,
) -> Result<String> {
    let url = reqwest::Url::parse(base)
        .map_err(|error| anyhow!("invalid AXON_SERVER_URL: {error}"))?
        .join("/v1/ingest/sessions/prepared")?;
    if url.scheme() != "https" && !url.host_str().is_some_and(is_loopback_host) {
        return Err(anyhow!(
            "--upload-to-server requires HTTPS unless AXON_SERVER_URL is loopback"
        ));
    }
    let body = serialize_remote_upload_body(request)?;
    if body.len() > MAX_UPLOAD_BODY_BYTES {
        return Err(anyhow!("prepared session upload body exceeds size limit"));
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let response = client
        .post(url)
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| anyhow!("remote prepared session upload response read failed: {error}"))?;
    if !status.is_success() {
        return Err(anyhow!(
            "remote prepared session upload failed: status={} body={}",
            status.as_u16(),
            redact_error_detail(&text)
        ));
    }
    if status != reqwest::StatusCode::ACCEPTED {
        return Err(anyhow!(
            "remote prepared session upload did not return 202 Accepted: status={} body={}",
            status.as_u16(),
            redact_error_detail(&text)
        ));
    }
    parse_remote_job_label(&text)
        .ok_or_else(|| anyhow!("remote prepared session upload response missing job_id"))
}

fn serialize_remote_upload_body(
    request: crate::sessions::IngestSessionsPreparedRequest,
) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&redact_remote_prepared_request(
        request,
    ))?)
}

pub fn redact_remote_prepared_request(
    request: crate::sessions::IngestSessionsPreparedRequest,
) -> crate::sessions::IngestSessionsPreparedRequest {
    crate::sessions::IngestSessionsPreparedRequest {
        docs: request
            .docs
            .into_iter()
            .map(redact_remote_prepared_doc)
            .collect(),
        project: request.project,
        collection: request.collection,
    }
}

fn redact_remote_prepared_doc(
    mut doc: crate::sessions::PreparedSessionDoc,
) -> crate::sessions::PreparedSessionDoc {
    let mut hasher = sha2::Sha256::new();
    Digest::update(&mut hasher, doc.url.as_bytes());
    Digest::update(&mut hasher, doc.session_file.as_bytes());
    let digest = hex::encode(Digest::finalize(hasher));
    let basename = Path::new(&doc.session_file)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("session")
        .to_string();
    doc.url = format!(
        "file:///redacted/{}/{}/{}",
        doc.session_platform,
        &digest[..16],
        basename
    );
    doc.session_file = basename;
    doc.extra = redact_remote_extra(doc.extra);
    doc
}

fn redact_remote_extra(extra: serde_json::Value) -> serde_json::Value {
    let Some(mut object) = extra.as_object().cloned() else {
        return serde_json::Value::Object(serde_json::Map::new());
    };
    for key in [
        "cwd",
        "path",
        "project_path",
        "session_file",
        "source_path",
        "transcript_path",
        "workspace",
        "workspace_path",
    ] {
        object.remove(key);
    }
    serde_json::Value::Object(object)
}

fn parse_remote_job_label(text: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| {
            value
                .pointer("/result/job_id")
                .or_else(|| value.pointer("/job_id"))
                .and_then(|job_id| job_id.as_str())
                .map(str::to_string)
        })
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::watch::validate::SessionProvider;
    use axon_core::config::SessionWatchConfig;
    use std::path::PathBuf;

    #[test]
    fn remote_upload_chunks_split_by_serialized_body_size() {
        let mut options = SessionWatchConfig {
            path: None,
            debounce: std::time::Duration::from_millis(1),
            settle: std::time::Duration::from_millis(1),
            max_retries: 1,
            max_batch_docs: 10,
            max_processing_concurrency: 1,
            rescan_cooldown: std::time::Duration::from_millis(1),
            initial_scan: false,
            upload_to_server: true,
            upload_server_url: None,
            upload_token: None,
            verbose_paths: false,
            json: false,
        };
        let items = vec![
            prepared_watch_doc("one.jsonl", "first"),
            prepared_watch_doc("two.jsonl", "second"),
            prepared_watch_doc("three.jsonl", "third"),
        ];
        let single_max = items
            .iter()
            .map(|item| remote_upload_body_len(&[item], None, None).unwrap())
            .max()
            .unwrap();
        let two_len = remote_upload_body_len(&[&items[0], &items[1]], None, None).unwrap();
        let target = single_max.max(1);

        assert!(two_len > target);
        let chunks = watch_upload_chunks_with_target(&items, &options, None, None, target).unwrap();
        assert_eq!(
            chunks.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![1, 1, 1]
        );

        options.max_batch_docs = 2;
        let chunks =
            watch_upload_chunks_with_target(&items, &options, None, None, usize::MAX).unwrap();
        assert_eq!(chunks.iter().map(Vec::len).collect::<Vec<_>>(), vec![2, 1]);
    }

    fn prepared_watch_doc(name: &str, text: &str) -> PreparedWatchDoc {
        let path = PathBuf::from(format!("/tmp/{name}"));
        let validated = ValidatedSessionPath {
            canonical: path.clone(),
            provider: SessionProvider::Codex,
            relative: PathBuf::from(name),
            basename: name.to_string(),
            redacted_display: format!("codex:{name}"),
            path_hash: format!("hash-{name}"),
        };
        let meta = SessionFileMetadata {
            canonical: path,
            path_hash: validated.path_hash.clone(),
            provider: "codex".to_string(),
            basename: name.to_string(),
            redacted_display: validated.redacted_display.clone(),
            file_size: text.len() as u64,
            file_mtime_ms: 1,
        };
        (
            0,
            validated,
            meta,
            crate::sessions::PreparedSessionDoc {
                url: format!("file:///tmp/{name}"),
                title: Some(name.to_string()),
                text: text.to_string(),
                session_platform: "codex".to_string(),
                session_project: None,
                session_date: None,
                session_turn_count: Some(1),
                session_file: name.to_string(),
                extra: serde_json::json!({}),
            },
            Some(format!("content-{name}")),
        )
    }
}
