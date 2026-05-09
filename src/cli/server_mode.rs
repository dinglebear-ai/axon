mod plan;

use crate::cli;
use crate::core::config::{CommandKind, Config};
use crate::core::ui::{accent, muted, primary, success, symbol_for_status};
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
    render_server_result(cfg, plan.label, &result)?;
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

fn render_server_result(
    cfg: &Config,
    label: &'static str,
    result: &serde_json::Value,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(());
    }

    match cfg.command {
        CommandKind::Status => render_server_status(result),
        CommandKind::Crawl => render_server_job_command("Crawl", result),
        CommandKind::Extract => render_server_job_command("Extract", result),
        CommandKind::Embed => render_server_job_command("Embed", result),
        CommandKind::Ingest | CommandKind::Sessions => render_server_job_command("Ingest", result),
        CommandKind::Scrape => render_server_payload("Scrape", result),
        CommandKind::Screenshot => render_server_payload("Screenshot", result),
        _ => {
            println!("{} {}", success("✓ Server mode"), label);
            println!("{}", serde_json::to_string_pretty(result)?);
        }
    }
    Ok(())
}

fn render_server_status(result: &serde_json::Value) {
    println!("{}", primary("Axon Status (server mode)"));
    if let Some(totals) = result.get("totals").and_then(|value| value.as_object()) {
        for name in ["crawl", "extract", "embed", "ingest"] {
            let count = totals
                .get(name)
                .and_then(|value| value.as_i64())
                .unwrap_or(0);
            println!("  {} {} total", muted(&format!("{name:<7}")), count);
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string())
        );
    }
}

fn render_server_job_command(title: &str, result: &serde_json::Value) {
    let status = result
        .get("status")
        .or_else(|| result.get("job").and_then(|job| job.get("status")))
        .and_then(|value| value.as_str())
        .unwrap_or("pending");
    println!(
        "{} {}",
        symbol_for_status(status),
        primary(&format!("{title} request handled by server"))
    );
    for id in job_ids_from_result(result) {
        println!("  {} {}", muted("Job:"), accent(&id));
    }
    if job_ids_from_result(result).is_empty() {
        println!(
            "{}",
            serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string())
        );
    }
}

fn render_server_payload(title: &str, result: &serde_json::Value) {
    println!(
        "{} {}",
        success("✓"),
        primary(&format!("{title} handled by server"))
    );
    if let Some(url) = result
        .get("url")
        .or_else(|| result.get("data").and_then(|data| data.get("url")))
        .and_then(|value| value.as_str())
    {
        println!("  {} {}", muted("URL:"), accent(url));
    }
    if let Some(handle) = result.get("artifact_handle").or_else(|| {
        result
            .get("data")
            .and_then(|data| data.get("artifact_handle"))
    }) && let Some(path) = handle.get("relative_path").and_then(|value| value.as_str())
    {
        println!("  {} {}", muted("Artifact:"), accent(path));
    }
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
                render_server_result(cfg, "job status", &result)?;
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
mod tests {
    use super::*;

    fn cfg(command: CommandKind, positional: &[&str]) -> Config {
        let mut cfg = Config::test_default();
        cfg.command = command;
        cfg.positional = positional.iter().map(|value| value.to_string()).collect();
        cfg.server_url = Some(reqwest::Url::parse("http://127.0.0.1:8001").unwrap());
        cfg
    }

    #[test]
    fn client_server_dispatch_routes_stateful_commands_to_server_client() {
        for command in [
            CommandKind::Status,
            CommandKind::Scrape,
            CommandKind::Crawl,
            CommandKind::Extract,
            CommandKind::Embed,
            CommandKind::Ingest,
            CommandKind::Sessions,
            CommandKind::Screenshot,
        ] {
            let cfg = cfg(command, &["https://example.com"]);
            assert_eq!(
                client_server_dispatch(&cfg),
                ClientServerDispatch::Server,
                "{command:?} should use ServerClient"
            );
        }
    }

    #[test]
    fn client_server_dispatch_explicit_local_mode_uses_local_paths() {
        let mut cfg = cfg(CommandKind::Crawl, &["https://example.com"]);
        cfg.local_mode = true;

        assert_eq!(client_server_dispatch(&cfg), ClientServerDispatch::Local);
    }

    #[test]
    fn client_server_dispatch_query_only_commands_remain_local() {
        for command in [
            CommandKind::Query,
            CommandKind::Retrieve,
            CommandKind::Search,
            CommandKind::Research,
            CommandKind::Sources,
            CommandKind::Domains,
            CommandKind::Stats,
        ] {
            let cfg = cfg(command, &["test"]);
            assert_eq!(
                client_server_dispatch(&cfg),
                ClientServerDispatch::Local,
                "{command:?} should remain local"
            );
        }
    }

    #[tokio::test]
    async fn client_server_dispatch_dead_server_fails_before_local_scrape_write() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind unused port");
        let addr = listener.local_addr().expect("local addr");
        drop(listener);

        let temp = tempfile::TempDir::new().expect("tempdir");
        let output = temp.path().join("scrape.md");
        let mut cfg = cfg(CommandKind::Scrape, &["https://example.com"]);
        cfg.server_url = Some(reqwest::Url::parse(&format!("http://{addr}")).unwrap());
        cfg.output_path = Some(output.clone());

        let err = run_server_mode_command(&cfg)
            .await
            .expect_err("dead server should fail");
        assert!(
            err.to_string().contains("start `axon serve`") && err.to_string().contains("--local"),
            "unexpected error: {err}"
        );
        assert!(
            !output.exists(),
            "server-mode dead-server path must not create local scrape output"
        );
    }

    #[test]
    fn embed_server_mode_rejects_host_local_paths() {
        assert!(server_mode_rejects_host_local_embed_input("./README.md"));
        assert!(server_mode_rejects_host_local_embed_input("/tmp/README.md"));
        assert!(server_mode_rejects_host_local_embed_input("../README.md"));
    }

    #[test]
    fn embed_server_mode_allows_url_and_text_inputs() {
        assert!(!server_mode_rejects_host_local_embed_input(
            "https://example.com/docs"
        ));
        assert!(!server_mode_rejects_host_local_embed_input(
            "plain text to embed"
        ));
    }

    #[test]
    fn embed_server_mode_plan_fails_clearly_for_host_local_path() {
        let cfg = cfg(CommandKind::Embed, &["./README.md"]);

        let err = plan::embed_server_action_plan(&cfg).expect_err("local path should fail");
        assert!(
            err.to_string()
                .contains("server mode does not accept host-local embed paths yet"),
            "unexpected error: {err}"
        );
    }
}
