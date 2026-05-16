mod plan;
mod render;

use crate::cli;
use crate::core::config::{CommandKind, Config};
use crate::mcp::schema::AxonRequest;
use crate::services::types::{ClientActionRequest, ClientActionResponse};
use std::error::Error;
use std::path::Path;
use std::time::{Duration, Instant};
use uuid::Uuid;

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

#[derive(Debug)]
struct ServerActionPlan {
    action: AxonRequest,
    label: &'static str,
    poll_family: Option<ServerJobFamily>,
}

pub(crate) fn client_server_dispatch(cfg: &Config) -> ClientServerDispatch {
    if cfg.local_mode || cfg.server_url.is_none() {
        return ClientServerDispatch::Local;
    }
    if is_server_routed_command(cfg.command) {
        ClientServerDispatch::Server
    } else {
        ClientServerDispatch::Local
    }
}

fn is_server_routed_command(command: CommandKind) -> bool {
    matches!(
        command,
        CommandKind::Status
            | CommandKind::Scrape
            | CommandKind::Crawl
            | CommandKind::Extract
            | CommandKind::Embed
            | CommandKind::Ingest
            | CommandKind::Sessions
            | CommandKind::Screenshot
    )
}

pub(crate) async fn run_server_mode_command(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let server_url = cfg
        .server_url
        .clone()
        .ok_or("server mode requires --server-url or AXON_SERVER_URL")?;
    let plan = plan::server_action_plan(cfg)?;
    let client = cli::client::ServerClient::new(server_url)?;
    let request = ClientActionRequest {
        request_id: Uuid::new_v4().to_string(),
        action: plan.action,
    };
    let response = post_server_action(&client, &request, plan.label).await?;
    let result = server_action_result(response)?;
    render::render_server_result(cfg, plan.label, &result)?;
    if cfg.wait
        && let Some(family) = plan.poll_family
    {
        poll_server_jobs(cfg, &client, family, &result).await?;
    }
    Ok(())
}

async fn post_server_action(
    client: &cli::client::ServerClient,
    request: &ClientActionRequest,
    label: &'static str,
) -> Result<ClientActionResponse, Box<dyn Error>> {
    client
        .post_action(request)
        .await
        .map_err(|err| server_client_error(label, err))
}

fn server_action_result(
    response: ClientActionResponse,
) -> Result<serde_json::Value, Box<dyn Error>> {
    if response.ok {
        return Ok(response.result.unwrap_or(serde_json::Value::Null));
    }
    let err = response
        .error
        .map(|error| {
            let hint = error
                .hint
                .map(|hint| format!(" Hint: {hint}"))
                .unwrap_or_default();
            format!("server mode failed: {}.{}", error.message, hint)
        })
        .unwrap_or_else(|| "server mode failed with an empty error envelope".to_string());
    Err(err.into())
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
    client: &cli::client::ServerClient,
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
            let request = ClientActionRequest {
                request_id: Uuid::new_v4().to_string(),
                action: plan::status_action_for_family(family, &job_id),
            };
            let response = post_server_action(client, &request, "job status").await?;
            let result = server_action_result(response)?;
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
