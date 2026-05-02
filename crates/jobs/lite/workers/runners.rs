use std::collections::HashMap;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::Notify;

use crate::crates::core::config::Config;
use crate::crates::core::ui::{accent, symbol_for_status};
use crate::crates::jobs::backend::{JobPayload, lift_err};
#[cfg(test)]
use crate::crates::jobs::lite::config_snapshot::ingest_config_json;
use crate::crates::jobs::lite::config_snapshot::{
    apply_lite_config_snapshot, decode_ingest_job_config, lite_config_snapshot_json,
};
use crate::crates::jobs::lite::ops::enqueue_job;

pub(super) type JobResult =
    Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;

pub(super) async fn run_crawl_job_lite(
    pool: &SqlitePool,
    cfg: &Config,
    id: uuid::Uuid,
    embed_notify: Option<Arc<Notify>>,
) -> JobResult {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT url, config_json FROM axon_crawl_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((url, config_json)) = row else {
        tracing::warn!(id = %id, table = "axon_crawl_jobs", "job row not found at execution time, may have been deleted mid-run");
        return Ok(None);
    };
    let effective_cfg = apply_lite_config_snapshot(cfg, &config_json).map_err(lift_err)?;

    crate::crates::core::http::validate_url(&url).map_err(lift_err)?;

    // Derive a per-job output directory to prevent concurrent crawls from clobbering each other.
    let job_output_dir = crate::crates::services::crawl::predict_crawl_output_dir(
        &effective_cfg.output_dir,
        &url,
        &id.to_string(),
    );

    let (summary, _) = crate::crates::crawl::engine::run_crawl_once(
        &effective_cfg,
        &url,
        effective_cfg.render_mode,
        &job_output_dir,
        None,
        effective_cfg.discover_sitemaps,
        Arc::new(HashMap::new()),
        Some(&id.to_string()),
    )
    .await
    .map_err(lift_err)?;

    // Auto-enqueue embed job for the crawled output when embedding is enabled.
    let embed_job_id = if effective_cfg.embed && summary.markdown_files > 0 {
        let markdown_dir = job_output_dir
            .join("markdown")
            .to_string_lossy()
            .to_string();
        match enqueue_job(
            pool,
            &JobPayload::Embed {
                input: markdown_dir,
                config_json: lite_config_snapshot_json(&effective_cfg).map_err(lift_err)?,
            },
        )
        .await
        {
            Ok(eid) => {
                if let Some(notify) = &embed_notify {
                    notify.notify_one();
                }
                Some(eid.to_string())
            }
            Err(e) => {
                tracing::warn!("lite crawl worker: failed to enqueue embed job: {e}");
                None
            }
        }
    } else {
        None
    };

    if !effective_cfg.json_output && !effective_cfg.quiet {
        eprintln!(
            "{} crawl completed {} pages={} markdown={} thin={} errors={} elapsed={} job={} output={}",
            symbol_for_status("completed"),
            accent(&url),
            summary.pages_seen,
            summary.markdown_files,
            summary.thin_pages,
            summary.error_pages,
            format_elapsed_ms(summary.elapsed_ms),
            id,
            job_output_dir.join("markdown").display()
        );
        if let Some(embed_job_id) = &embed_job_id {
            eprintln!("  embed queued job={embed_job_id}");
        } else if effective_cfg.embed {
            eprintln!("  embed skipped no markdown output");
        } else {
            eprintln!("  embed disabled");
        }
    }

    Ok(Some(serde_json::json!({
        "url": url,
        // CLI/MCP `crawl status` reads these field names (see
        // crates/cli/commands/crawl/subcommands.rs:print_status_metrics).
        // Keep both legacy names (`pages_seen`, `markdown_files`) and the
        // canonical names (`pages_crawled`, `md_created`) so older consumers
        // still work.
        "pages_crawled": summary.pages_seen,
        "pages_seen": summary.pages_seen,
        "md_created": summary.markdown_files,
        "markdown_files": summary.markdown_files,
        "pages_discovered": summary.pages_discovered,
        "thin_md": summary.thin_pages,
        "error_pages": summary.error_pages,
        "waf_blocked_pages": summary.waf_blocked_pages,
        "elapsed_ms": summary.elapsed_ms,
        "embed_job_id": embed_job_id,
    })))
}

fn format_elapsed_ms(elapsed_ms: u128) -> String {
    if elapsed_ms >= 1_000 {
        format!("{:.1}s", elapsed_ms as f64 / 1_000.0)
    } else {
        format!("{elapsed_ms}ms")
    }
}

pub(super) async fn run_embed_job_lite(
    pool: &SqlitePool,
    cfg: &Config,
    id: uuid::Uuid,
) -> JobResult {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT input_text, config_json FROM axon_embed_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((input, config_json)) = row else {
        tracing::warn!(id = %id, table = "axon_embed_jobs", "job row not found at execution time, may have been deleted mid-run");
        return Ok(None);
    };

    let mut worker_cfg = apply_lite_config_snapshot(cfg, &config_json).map_err(lift_err)?;
    worker_cfg.json_output = false;
    let summary = crate::crates::vector::ops::embed_path_native(&worker_cfg, &input)
        .await
        .map_err(lift_err)?;

    Ok(Some(serde_json::json!({
        "input": input,
        "collection": worker_cfg.collection,
        "docs_embedded": summary.docs_embedded,
        "chunks_embedded": summary.chunks_embedded,
    })))
}

pub(super) async fn run_extract_job_lite(
    pool: &SqlitePool,
    cfg: &Config,
    id: uuid::Uuid,
) -> JobResult {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT urls_json, config_json FROM axon_extract_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((urls_json, config_json)) = row else {
        tracing::warn!(id = %id, table = "axon_extract_jobs", "job row not found at execution time, may have been deleted mid-run");
        return Ok(None);
    };
    let effective_cfg = apply_lite_config_snapshot(cfg, &config_json).map_err(lift_err)?;

    let urls: Vec<String> = serde_json::from_str(&urls_json).map_err(
        |e| -> Box<dyn std::error::Error + Send + Sync> {
            format!("invalid urls_json in extract job {id}: {e}").into()
        },
    )?;

    let engine = Arc::new(
        crate::crates::core::content::DeterministicExtractionEngine::with_default_parsers(),
    );
    let mut total_items = 0usize;
    for url in &urls {
        let wcfg = crate::crates::core::content::ExtractWebConfig {
            start_url: url.clone(),
            prompt: effective_cfg.query.clone().unwrap_or_default(),
            limit: effective_cfg.max_pages,
            openai_base_url: effective_cfg.openai_base_url.clone(),
            openai_api_key: effective_cfg.openai_api_key.clone(),
            openai_model: effective_cfg.openai_model.clone(),
            acp_adapter_cmd: effective_cfg.acp_adapter_cmd.clone(),
            acp_adapter_args: effective_cfg.acp_adapter_args.clone(),
            custom_headers: effective_cfg.custom_headers.clone(),
            render_mode: effective_cfg.render_mode,
            chrome_remote_url: effective_cfg.chrome_remote_url.clone(),
            chrome_stealth: effective_cfg.chrome_stealth,
            chrome_anti_bot: effective_cfg.chrome_anti_bot,
            chrome_intercept: effective_cfg.chrome_intercept,
            bypass_csp: effective_cfg.bypass_csp,
            accept_invalid_certs: effective_cfg.accept_invalid_certs,
            request_timeout_ms: effective_cfg.request_timeout_ms,
            fetch_retries: effective_cfg.fetch_retries,
            user_agent: effective_cfg.chrome_user_agent.clone(),
            chrome_network_idle_timeout_secs: effective_cfg.chrome_network_idle_timeout_secs,
        };
        let run = crate::crates::core::content::run_extract_with_engine(wcfg, Arc::clone(&engine))
            .await
            .map_err(lift_err)?;
        total_items += run.results.len();
    }

    Ok(Some(serde_json::json!({
        "urls": urls.len(),
        "total_items": total_items,
    })))
}

pub(super) async fn run_ingest_job_lite(
    pool: &SqlitePool,
    cfg: &Config,
    id: uuid::Uuid,
) -> JobResult {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT config_json FROM axon_ingest_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((config_json,)) = row else {
        tracing::warn!(id = %id, table = "axon_ingest_jobs", "job row not found at execution time, may have been deleted mid-run");
        return Ok(None);
    };

    let (source, effective_cfg) = decode_ingest_job_config(cfg, &config_json).map_err(|e| {
        let preview: String = config_json.chars().take(120).collect();
        format!("ingest job {id}: malformed config_json: {e} (preview: {preview:?})")
    })?;

    let result = match source {
        crate::crates::jobs::ingest::IngestSource::Github {
            repo,
            include_source,
        } => {
            let (owner, repo_name) = crate::crates::ingest::github::parse_github_repo(&repo)
                .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
                    format!("invalid github target: {repo}").into()
                })?;
            let mut github_cfg = effective_cfg.clone();
            github_cfg.github_include_source = include_source;
            crate::crates::services::ingest::ingest_github(&github_cfg, &owner, &repo_name, None)
                .await
                .map_err(lift_err)?
        }
        crate::crates::jobs::ingest::IngestSource::Reddit { target } => {
            crate::crates::services::ingest::ingest_reddit(&effective_cfg, &target, None)
                .await
                .map_err(lift_err)?
        }
        crate::crates::jobs::ingest::IngestSource::Youtube { target } => {
            crate::crates::services::ingest::ingest_youtube(&effective_cfg, &target, None)
                .await
                .map_err(lift_err)?
        }
        crate::crates::jobs::ingest::IngestSource::Sessions {
            sessions_claude,
            sessions_codex,
            sessions_gemini,
            sessions_project,
        } => {
            let mut sessions_cfg = effective_cfg.clone();
            sessions_cfg.sessions_claude = sessions_claude;
            sessions_cfg.sessions_codex = sessions_codex;
            sessions_cfg.sessions_gemini = sessions_gemini;
            sessions_cfg.sessions_project = sessions_project;
            crate::crates::services::ingest::ingest_sessions(&sessions_cfg, None)
                .await
                .map_err(lift_err)?
        }
    };

    Ok(Some(result.payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::RenderMode;
    use crate::crates::jobs::ingest::IngestSource;
    use std::path::PathBuf;

    #[test]
    fn lite_config_snapshot_applies_submitted_non_secret_values() {
        let mut submitted = Config::test_default();
        submitted.collection = "submitted_collection".to_string();
        submitted.output_dir = PathBuf::from("/tmp/axon-submitted");
        submitted.render_mode = RenderMode::Chrome;
        submitted.max_pages = 37;
        submitted.max_depth = 4;
        submitted.embed = false;
        submitted.query = Some("submitted prompt".to_string());
        submitted.request_timeout_ms = Some(12_345);
        submitted.fetch_retries = 7;
        submitted.qdrant_url = "http://submitted-qdrant:6333".to_string();
        submitted.tei_url = "http://submitted-tei:80".to_string();
        submitted.openai_model = "submitted-model".to_string();
        submitted.openai_api_key = "submitted-secret".to_string();
        submitted.chrome_proxy = Some("http://submitted-proxy:8080".to_string());
        submitted.acp_adapter_args = Some("--model|submitted".to_string());
        submitted.custom_headers = vec!["Authorization: Bearer submitted".to_string()];

        let mut worker = Config::test_default();
        worker.collection = "worker_collection".to_string();
        worker.output_dir = PathBuf::from("/tmp/axon-worker");
        worker.render_mode = RenderMode::Http;
        worker.max_pages = 1;
        worker.max_depth = 1;
        worker.embed = true;
        worker.query = Some("worker prompt".to_string());
        worker.request_timeout_ms = Some(999);
        worker.fetch_retries = 1;
        worker.qdrant_url = "http://worker-qdrant:6333".to_string();
        worker.tei_url = "http://worker-tei:80".to_string();
        worker.openai_model = "worker-model".to_string();
        worker.openai_api_key = "worker-secret".to_string();
        worker.chrome_proxy = Some("http://worker-proxy:8080".to_string());
        worker.acp_adapter_args = Some("--model|worker".to_string());
        worker.custom_headers = vec!["Authorization: Bearer worker".to_string()];

        let config_json = lite_config_snapshot_json(&submitted).expect("encode snapshot");
        let effective = apply_lite_config_snapshot(&worker, &config_json).expect("apply snapshot");

        assert_eq!(effective.collection, "submitted_collection");
        assert_eq!(effective.output_dir, PathBuf::from("/tmp/axon-submitted"));
        assert_eq!(effective.render_mode, RenderMode::Chrome);
        assert_eq!(effective.max_pages, 37);
        assert_eq!(effective.max_depth, 4);
        assert!(!effective.embed);
        assert_eq!(effective.query.as_deref(), Some("submitted prompt"));
        assert_eq!(effective.request_timeout_ms, Some(12_345));
        assert_eq!(effective.fetch_retries, 7);
        assert_eq!(effective.qdrant_url, "http://submitted-qdrant:6333");
        assert_eq!(effective.tei_url, "http://submitted-tei:80");
        assert_eq!(effective.openai_model, "submitted-model");
        assert_eq!(
            effective.chrome_proxy.as_deref(),
            Some("http://submitted-proxy:8080")
        );
        assert_eq!(
            effective.acp_adapter_args.as_deref(),
            Some("--model|submitted")
        );

        assert_eq!(effective.openai_api_key, "worker-secret");
        assert_eq!(
            effective.custom_headers,
            vec!["Authorization: Bearer submitted".to_string()]
        );
    }

    #[test]
    fn lite_config_snapshot_exactly_replays_submitted_none_options() {
        let mut submitted = Config::test_default();
        submitted.output_path = None;
        submitted.request_timeout_ms = None;
        submitted.chrome_wait_for_selector = None;
        submitted.acp_adapter_args = None;

        let mut worker = Config::test_default();
        worker.output_path = Some(PathBuf::from("/tmp/worker-output.md"));
        worker.request_timeout_ms = Some(999);
        worker.chrome_wait_for_selector = Some("#app".to_string());
        worker.acp_adapter_args = Some("--model|worker".to_string());

        let config_json = lite_config_snapshot_json(&submitted).expect("encode snapshot");
        let effective = apply_lite_config_snapshot(&worker, &config_json).expect("apply snapshot");

        assert_eq!(effective.output_path, None);
        assert_eq!(effective.request_timeout_ms, None);
        assert_eq!(effective.chrome_wait_for_selector, None);
        assert_eq!(effective.acp_adapter_args, None);
    }

    #[test]
    fn lite_config_snapshot_does_not_serialize_credential_bearing_endpoint_urls() {
        let mut submitted = Config::test_default();
        submitted.tei_url = "http://user:secret@tei.example/embed?token=abc#frag".to_string();
        submitted.qdrant_url = "http://qdrant.example:6333?api_key=secret".to_string();
        submitted.openai_base_url = "https://llm.example/v1?token=secret".to_string();
        submitted.acp_ws_url = Some("wss://axon.example/ws?token=secret".to_string());

        let mut worker = Config::test_default();
        worker.tei_url = "http://worker-tei:80".to_string();
        worker.qdrant_url = "http://worker-qdrant:6333".to_string();
        worker.openai_base_url = "http://worker-llm/v1".to_string();
        worker.acp_ws_url = Some("wss://worker/ws".to_string());

        let config_json = lite_config_snapshot_json(&submitted).expect("encode snapshot");
        assert!(!config_json.contains("secret"));
        assert!(!config_json.contains("token=abc"));
        assert!(!config_json.contains("api_key"));
        assert!(!config_json.contains("user:"));

        let effective = apply_lite_config_snapshot(&worker, &config_json).expect("apply snapshot");
        assert_eq!(effective.tei_url, "http://worker-tei:80");
        assert_eq!(effective.qdrant_url, "http://worker-qdrant:6333");
        assert_eq!(effective.openai_base_url, "http://worker-llm/v1");
        assert_eq!(effective.acp_ws_url.as_deref(), Some("wss://worker/ws"));
    }

    #[test]
    fn ingest_job_config_preserves_source_and_supports_legacy_rows() {
        let mut submitted = Config::test_default();
        submitted.collection = "submitted_collection".to_string();
        let source = IngestSource::Github {
            repo: "owner/repo".to_string(),
            include_source: false,
        };

        let mut worker = Config::test_default();
        worker.collection = "worker_collection".to_string();

        let config_json = ingest_config_json(&submitted, &source).expect("encode ingest config");
        let (decoded_source, effective) =
            decode_ingest_job_config(&worker, &config_json).expect("decode ingest config");
        assert!(matches!(
            decoded_source,
            IngestSource::Github {
                ref repo,
                include_source: false,
            } if repo == "owner/repo"
        ));
        assert_eq!(effective.collection, "submitted_collection");

        let legacy_json = serde_json::to_string(&source).expect("encode legacy source");
        let (legacy_source, legacy_effective) =
            decode_ingest_job_config(&worker, &legacy_json).expect("decode legacy ingest config");
        assert!(matches!(
            legacy_source,
            IngestSource::Github {
                ref repo,
                include_source: false,
            } if repo == "owner/repo"
        ));
        assert_eq!(legacy_effective.collection, "worker_collection");
    }
}
