mod plan;
mod render;

use crate::cli;
use crate::cli::route::{CommandRoute, plan_command_route};
use crate::core::config::{CommandKind, Config};
use std::error::Error;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientServerDispatch {
    Local,
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerJobFamily {
    Crawl,
    Extract,
    Embed,
    Ingest,
}

pub(crate) fn client_server_dispatch(cfg: &Config) -> ClientServerDispatch {
    if matches!(
        plan_command_route(cfg, &cfg.positional),
        Ok(plan) if plan.route == CommandRoute::PreferServer
    ) {
        ClientServerDispatch::Server
    } else {
        ClientServerDispatch::Local
    }
}

pub(crate) async fn run_server_mode_command(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let server_url = cfg
        .server_url
        .clone()
        .ok_or("server mode requires AXON_SERVER_URL")?;
    if matches!(cfg.command, CommandKind::Sessions) && !has_async_job_lifecycle_subcommand(cfg) {
        return run_server_mode_sessions(cfg, server_url).await;
    }
    let plan = plan::server_rest_plan(cfg)?;
    let client = Arc::new(cli::client::ServerClient::new(server_url)?);
    let result = if should_stream_server_result(cfg) {
        request_server_streaming_rest(&client, &plan).await?
    } else {
        request_server_rest(&client, &plan).await?
    };
    if cfg.wait
        && let Some(family) = plan.poll_family
    {
        poll_server_jobs(cfg, &client, family, &result).await?;
    } else {
        render::render_server_result(cfg, plan.label, &result)?;
    }
    Ok(())
}

fn has_async_job_lifecycle_subcommand(cfg: &Config) -> bool {
    matches!(
        cfg.positional.first().map(String::as_str),
        Some("status" | "errors" | "list" | "cleanup" | "recover" | "clear" | "cancel" | "worker")
    )
}

async fn run_server_mode_sessions(
    cfg: &Config,
    server_url: reqwest::Url,
) -> Result<(), Box<dyn Error>> {
    use crate::ingest::sessions::{IngestSessionsPreparedRequest, MAX_PREPARED_SESSION_DOCS};

    let client = Arc::new(cli::client::ServerClient::new(server_url)?);
    let request = crate::ingest::sessions::prepare_sessions_request(cfg).await?;
    if request.docs.is_empty() {
        if cfg.json_output {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "skipped",
                    "reason": "no session documents matched",
                    "chunks_embedded": 0,
                }))?
            );
        } else {
            eprintln!("sessions: no session documents matched; nothing to upload");
        }
        return Ok(());
    }

    let collection = request.collection.clone();
    let project = request.project.clone();
    let total = request.docs.len();
    let batch_count = total.div_ceil(MAX_PREPARED_SESSION_DOCS);

    if !cfg.json_output {
        let noun = if batch_count == 1 { "batch" } else { "batches" };
        eprintln!("sessions: queueing {total} docs → {batch_count} {noun}");
    }

    let mut all_job_ids: Vec<String> = Vec::new();
    for batch_docs in request.docs.chunks(MAX_PREPARED_SESSION_DOCS) {
        let batch = IngestSessionsPreparedRequest {
            docs: batch_docs.to_vec(),
            project: project.clone(),
            collection: collection.clone(),
        };
        let result: serde_json::Value = client
            .post_json("/v1/ingest/sessions/prepared", &batch, "sessions")
            .await
            .map_err(|err| server_client_error("sessions", err))?;
        if let Some(job_id) = result.get("job_id").and_then(|v| v.as_str()) {
            all_job_ids.push(job_id.to_string());
        }
    }

    let combined_result = serde_json::json!({ "job_ids": all_job_ids });
    if cfg.json_output {
        return render::render_server_result(cfg, "sessions", &combined_result);
    }
    if cfg.wait {
        poll_server_jobs(cfg, &client, ServerJobFamily::Ingest, &combined_result).await
    } else {
        poll_sessions_progress(&client, &all_job_ids, batch_count).await
    }
}

fn should_stream_server_result(cfg: &Config) -> bool {
    if cfg.json_output {
        return false;
    }
    match cfg.command {
        CommandKind::Ask => cfg.ask_stream && !cfg.ask_explain,
        CommandKind::Research | CommandKind::Summarize => true,
        _ => false,
    }
}

async fn request_server_rest(
    client: &Arc<cli::client::ServerClient>,
    plan: &plan::ServerRestPlan,
) -> Result<serde_json::Value, Box<dyn Error>> {
    match plan.method {
        "GET" => client
            .get_json(&plan.path, plan.label)
            .await
            .map_err(|err| server_client_error(plan.label, err)),
        "POST" => client
            .post_json(&plan.path, &plan.body, plan.label)
            .await
            .map_err(|err| server_client_error(plan.label, err)),
        "DELETE" => client
            .delete_json(&plan.path, plan.label)
            .await
            .map_err(|err| server_client_error(plan.label, err)),
        method => Err(format!("unsupported server mode method: {method}").into()),
    }
}

async fn request_server_streaming_rest(
    client: &Arc<cli::client::ServerClient>,
    plan: &plan::ServerRestPlan,
) -> Result<serde_json::Value, Box<dyn Error>> {
    if plan.method != "POST" {
        return request_server_rest(client, plan).await;
    }
    let stream_path = format!("{}/stream", plan.path.trim_end_matches('/'));
    let mut stdout = std::io::stdout();
    let mut streamed = false;
    let result = client
        .post_json_sse(&stream_path, &plan.body, plan.label, |delta| {
            streamed = true;
            let _ = stdout.write_all(delta.as_bytes());
            let _ = stdout.flush();
        })
        .await
        .map_err(|err| server_client_error(plan.label, err))?;
    if streamed {
        let _ = writeln!(stdout);
    }
    Ok(result)
}

fn server_client_error(label: &'static str, err: cli::client::ServerClientError) -> Box<dyn Error> {
    use cli::client::ServerClientErrorKind;
    let hint = match err.kind() {
        ServerClientErrorKind::Auth => {
            "Hint: AXON_MCP_HTTP_TOKEN token mismatch; use the same token as axon serve."
        }
        ServerClientErrorKind::Connect => {
            "Hint: start `axon serve` or use explicit local mode with `--local`."
        }
        ServerClientErrorKind::VersionMismatch | ServerClientErrorKind::Decode => {
            "Hint: rebuild/restart the canonical server so the client/server schemas match."
        }
        ServerClientErrorKind::CleartextBearer => {
            "Hint: use HTTPS, loopback, or explicitly allow insecure server mode."
        }
        ServerClientErrorKind::Status | ServerClientErrorKind::BuildClient => {
            "Hint: inspect the axon serve logs for the first-party action API."
        }
    };
    format!("server mode {label} failed: {err}\n{hint}").into()
}

async fn poll_server_jobs(
    cfg: &Config,
    client: &Arc<cli::client::ServerClient>,
    family: ServerJobFamily,
    start_result: &serde_json::Value,
) -> Result<(), Box<dyn Error>> {
    let job_ids = job_ids_from_result(start_result);
    if job_ids.is_empty() {
        return Ok(());
    }

    let deadline = Instant::now() + Duration::from_secs(cli::client::SERVER_ACTION_TIMEOUT_SECS);
    for job_id in job_ids {
        loop {
            if Instant::now() >= deadline {
                return Err(format!("server mode wait timed out for job {job_id}").into());
            }
            let path = plan::status_path_for_family(family, &job_id);
            let result: serde_json::Value = client
                .get_json(&path, "job status")
                .await
                .map_err(|err| server_client_error("job status", err))?;
            if let Some(status) = job_status_from_result(&result)
                && matches!(status, "completed" | "failed" | "canceled")
            {
                render::render_server_result(cfg, "job status", &result)?;
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    Ok(())
}

async fn poll_sessions_progress(
    client: &Arc<cli::client::ServerClient>,
    job_ids: &[String],
    batch_count: usize,
) -> Result<(), Box<dyn Error>> {
    use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
    use tokio::sync::Mutex;

    if job_ids.is_empty() {
        return Ok(());
    }

    let mp = MultiProgress::new();
    let spinner_style = ProgressStyle::with_template("{spinner:.cyan} {prefix:.bold} {msg}")
        .unwrap_or_else(|_| ProgressStyle::default_spinner())
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", " "]);

    let bars: Vec<ProgressBar> = job_ids
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let pb = mp.add(ProgressBar::new_spinner());
            pb.set_style(spinner_style.clone());
            let label = if batch_count > 1 {
                format!("batch {}/{}", i + 1, batch_count)
            } else {
                "sessions".to_string()
            };
            pb.set_prefix(label);
            pb.set_message("pending");
            pb
        })
        .collect();

    let deadline = Instant::now() + Duration::from_secs(cli::client::SERVER_ACTION_TIMEOUT_SECS);
    let total_chunks = Arc::new(Mutex::new(0u64));
    let mut tasks = tokio::task::JoinSet::new();

    for (idx, job_id) in job_ids.iter().enumerate() {
        let client = Arc::clone(client);
        let pb = bars[idx].clone();
        let job_id = job_id.clone();
        let total_chunks = Arc::clone(&total_chunks);

        tasks.spawn(async move {
            let path = plan::status_path_for_family(ServerJobFamily::Ingest, &job_id);
            loop {
                if Instant::now() >= deadline {
                    pb.finish_with_message("timed out");
                    return Err::<(), Box<dyn Error + Send + Sync>>(
                        format!("timed out waiting for job {job_id}").into(),
                    );
                }

                pb.tick();
                let result: serde_json::Value = match client.get_json(&path, "job status").await {
                    Ok(v) => v,
                    Err(e) => {
                        pb.set_message(format!("poll error: {e}"));
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                };

                let job = result.get("job").unwrap_or(&result);
                let status = job
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                match status {
                    "completed" => {
                        let chunks = job
                            .get("result_json")
                            .and_then(|r| r.get("payload"))
                            .and_then(|p| p.get("chunks"))
                            .and_then(|c| c.as_u64())
                            .unwrap_or(0);
                        *total_chunks.lock().await += chunks;
                        pb.finish_with_message(format!("done — {chunks} chunks embedded"));
                        return Ok(());
                    }
                    "failed" => {
                        let err = job
                            .get("error_text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown error");
                        pb.finish_with_message(format!("failed: {err}"));
                        return Err(format!("job {job_id} failed: {err}").into());
                    }
                    "canceled" => {
                        pb.finish_with_message("canceled");
                        return Ok(());
                    }
                    "running" => pb.set_message("running"),
                    _ => pb.set_message("pending"),
                }

                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    let mut any_error: Option<String> = None;
    while let Some(outcome) = tasks.join_next().await {
        match outcome {
            Ok(Err(e)) => {
                if any_error.is_none() {
                    any_error = Some(e.to_string());
                }
            }
            Err(join_err) => {
                if any_error.is_none() {
                    any_error = Some(join_err.to_string());
                }
            }
            Ok(Ok(())) => {}
        }
    }

    let chunks = *total_chunks.lock().await;
    eprintln!("sessions: complete — {chunks} chunks embedded");

    if let Some(err) = any_error {
        Err(err.into())
    } else {
        Ok(())
    }
}

fn job_ids_from_result(result: &serde_json::Value) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(id) = result.get("job_id").and_then(|value| value.as_str()) {
        ids.push(id.to_string());
    }
    if let Some(values) = result.get("job_ids").and_then(|value| value.as_array()) {
        ids.extend(
            values
                .iter()
                .filter_map(|value| value.as_str().map(ToString::to_string)),
        );
    }
    if let Some(jobs) = result.get("jobs").and_then(|value| value.as_array()) {
        ids.extend(jobs.iter().filter_map(|job| {
            job.get("job_id")
                .or_else(|| job.get("id"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        }));
    }
    ids.sort();
    ids.dedup();
    ids
}

fn job_status_from_result(result: &serde_json::Value) -> Option<&str> {
    result
        .get("status")
        .or_else(|| result.get("job").and_then(|job| job.get("status")))
        .and_then(|value| value.as_str())
}

fn server_mode_rejects_host_local_embed_input(input: &str) -> bool {
    if input.trim().is_empty() {
        return true;
    }
    if input.starts_with("http://") || input.starts_with("https://") {
        return false;
    }
    let path = Path::new(input);
    path.is_absolute()
        || path.exists()
        || input.starts_with("./")
        || input.starts_with("../")
        || input.starts_with("~/")
        || input.contains(std::path::MAIN_SEPARATOR)
}

#[cfg(test)]
#[path = "server_mode_tests.rs"]
mod tests;
