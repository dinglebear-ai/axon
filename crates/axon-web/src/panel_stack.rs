use super::server::{AppState, HttpError, authorized};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use tokio::process::Command;

#[derive(Serialize)]
struct StackResponse {
    runtime_mode: &'static str,
    server_url: String,
    mcp_url: String,
    log_dir: String,
    compose_file: String,
    urls: Vec<StackUrlCheck>,
    checks: Vec<StackCheck>,
}

#[derive(Serialize)]
struct StackCheck {
    label: &'static str,
    status: &'static str,
    detail: String,
}

#[derive(Serialize)]
struct StackUrlCheck {
    label: &'static str,
    url: String,
    status: &'static str,
    detail: String,
}

pub(super) async fn stack_status(
    State((state, cfg)): State<(AppState, Arc<axon_core::config::Config>)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }

    let home =
        axon_core::paths::axon_home_dir().unwrap_or_else(|| std::path::PathBuf::from("~/.axon"));
    let compose_file = home.join("compose/docker-compose.yaml");
    let qdrant_ready_url = format!("{}/readyz", cfg.qdrant_url.trim_end_matches('/'));
    let tei_health_url = format!("{}/health", cfg.tei_url.trim_end_matches('/'));
    let chrome_url = cfg.chrome_remote_url.clone();
    let runtime_mode = StackRuntimeMode::detect();
    let server_host = browser_display_host(&cfg.mcp_http_host);
    let server_url = format!("http://{}:{}", server_host, cfg.mcp_http_port);
    let mcp_url = format!("{server_url}/mcp");
    let public_url = std::env::var("AXON_PUBLIC_URL")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty());
    let checks = stack_checks(
        runtime_mode,
        &compose_file,
        &qdrant_ready_url,
        &cfg.tei_url,
        chrome_url,
    )
    .await;
    let urls = url_checks(
        &server_url,
        &mcp_url,
        public_url.as_deref(),
        &qdrant_ready_url,
        &tei_health_url,
        cfg.chrome_remote_url.as_deref(),
    )
    .await;

    Json(StackResponse {
        runtime_mode: runtime_mode.as_str(),
        mcp_url,
        server_url,
        log_dir: home.join("logs").display().to_string(),
        compose_file: compose_file.display().to_string(),
        urls,
        checks,
    })
    .into_response()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StackRuntimeMode {
    Host,
    Container,
}

impl StackRuntimeMode {
    fn detect() -> Self {
        if axon_core::config::parse::docker::running_in_container() {
            Self::Container
        } else {
            Self::Host
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Container => "container",
        }
    }
}

async fn stack_checks(
    runtime_mode: StackRuntimeMode,
    compose_file: &std::path::Path,
    qdrant_url: &str,
    tei_url: &str,
    chrome_url: Option<String>,
) -> Vec<StackCheck> {
    let (qdrant, tei, chrome) = tokio::join!(
        http_check("Qdrant", qdrant_url),
        tei_check(tei_url),
        async {
            if let Some(chrome_url) = chrome_url.as_deref() {
                http_check("Chrome", chrome_url).await
            } else {
                check("Chrome", "warn", "AXON_CHROME_REMOTE_URL is unset")
            }
        },
    );

    let mut checks = host_prerequisite_checks(runtime_mode, compose_file).await;
    checks.extend([qdrant, tei, chrome, token_check(), oauth_check()]);
    checks
}

async fn url_checks(
    server_url: &str,
    mcp_url: &str,
    public_url: Option<&str>,
    qdrant_ready_url: &str,
    tei_health_url: &str,
    chrome_url: Option<&str>,
) -> Vec<StackUrlCheck> {
    let server_ready_url = format!("{}/readyz", server_url.trim_end_matches('/'));
    let public_ready_url = public_url.map(|url| format!("{}/readyz", url.trim_end_matches('/')));
    let chrome_target = chrome_url.map(str::to_string);

    let (server, mcp, public, qdrant, tei, chrome) = tokio::join!(
        http_url_check(
            "Panel / readyz",
            &server_ready_url,
            HttpExpectation::Success
        ),
        http_url_check("MCP endpoint", mcp_url, HttpExpectation::AnyResponse),
        async {
            match public_ready_url.as_deref() {
                Some(url) => http_url_check("Public URL", url, HttpExpectation::Success).await,
                None => url_check("Public URL", "", "skipped", "AXON_PUBLIC_URL is unset"),
            }
        },
        http_url_check("Qdrant readyz", qdrant_ready_url, HttpExpectation::Success),
        http_url_check("TEI health", tei_health_url, HttpExpectation::Success),
        async {
            match chrome_target.as_deref() {
                Some(url) => http_url_check("Chrome control", url, HttpExpectation::Success).await,
                None => url_check(
                    "Chrome control",
                    "",
                    "skipped",
                    "AXON_CHROME_REMOTE_URL is unset",
                ),
            }
        },
    );

    vec![server, mcp, public, qdrant, tei, chrome]
}

#[derive(Clone, Copy)]
enum HttpExpectation {
    Success,
    AnyResponse,
}

async fn http_url_check(
    label: &'static str,
    url: &str,
    expectation: HttpExpectation,
) -> StackUrlCheck {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(client) => client,
        Err(err) => return url_check(label, url, "error", err.to_string()),
    };

    match client.get(url).send().await {
        Ok(response) if response.status().is_success() => {
            url_check(label, url, "ok", format!("HTTP {}", response.status()))
        }
        Ok(response) if matches!(expectation, HttpExpectation::AnyResponse) => url_check(
            label,
            url,
            "ok",
            format!("reachable; HTTP {}", response.status()),
        ),
        Ok(response) => url_check(label, url, "error", format!("HTTP {}", response.status())),
        Err(err) => url_check(label, url, "error", err.to_string()),
    }
}

async fn host_prerequisite_checks(
    runtime_mode: StackRuntimeMode,
    compose_file: &std::path::Path,
) -> Vec<StackCheck> {
    match runtime_mode {
        StackRuntimeMode::Container => vec![
            skipped_host_check("Docker"),
            skipped_host_check("Docker Compose"),
            skipped_host_check("NVIDIA runtime"),
            skipped_host_check("Compose assets"),
            skipped_host_check("Gemini CLI"),
        ],
        StackRuntimeMode::Host => {
            let (docker, compose, nvidia, gemini) = tokio::join!(
                command_check("Docker", "docker", ["--version"]),
                command_check("Docker Compose", "docker", ["compose", "version"]),
                command_check(
                    "NVIDIA runtime",
                    "nvidia-smi",
                    ["--query-gpu=name", "--format=csv,noheader"],
                ),
                gemini_check()
            );
            vec![
                docker,
                compose,
                nvidia,
                compose_file_check(compose_file),
                gemini,
            ]
        }
    }
}

fn skipped_host_check(label: &'static str) -> StackCheck {
    check(
        label,
        "skipped",
        "host prerequisite check skipped from container-served panel",
    )
}

fn browser_display_host(bind_host: &str) -> &str {
    match bind_host {
        "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
        host => host,
    }
}

async fn command_check<const N: usize>(
    label: &'static str,
    program: &str,
    args: [&str; N],
) -> StackCheck {
    let result =
        axon_services::setup::diagnostics::check_command(program, args, Duration::from_secs(4))
            .await;
    match result.status {
        axon_services::setup::diagnostics::CommandStatus::Ok => check(label, "ok", result.detail),
        axon_services::setup::diagnostics::CommandStatus::Failed
        | axon_services::setup::diagnostics::CommandStatus::NotFound
        | axon_services::setup::diagnostics::CommandStatus::TimedOut => {
            check(label, "error", result.detail)
        }
    }
}

fn compose_file_check(path: &std::path::Path) -> StackCheck {
    if path.exists() {
        check("Compose assets", "ok", format!("found {}", path.display()))
    } else {
        check(
            "Compose assets",
            "warn",
            format!("missing {}; run axon setup init", path.display()),
        )
    }
}

async fn http_check(label: &'static str, url: &str) -> StackCheck {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(client) => client,
        Err(err) => return check(label, "error", err.to_string()),
    };
    match client.get(url).send().await {
        Ok(response) if response.status().is_success() => {
            check(label, "ok", format!("{url} returned {}", response.status()))
        }
        Ok(response) => check(
            label,
            "error",
            format!("{url} returned {}", response.status()),
        ),
        Err(err) => check(label, "error", err.to_string()),
    }
}

async fn tei_check(base_url: &str) -> StackCheck {
    let health_url = format!("{}/health", base_url.trim_end_matches('/'));
    let info_url = format!("{}/info", base_url.trim_end_matches('/'));
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(client) => client,
        Err(err) => return check("TEI / Qwen3", "error", err.to_string()),
    };
    match client.get(&health_url).send().await {
        Ok(response) if response.status().is_success() => {
            match client.get(&info_url).send().await {
                Ok(info) if info.status().is_success() => match info.text().await {
                    Ok(body) if qwen3_model_reported(&body) => {
                        check("TEI / Qwen3", "ok", "healthy; Qwen3 model reported")
                    }
                    Ok(_) => check(
                        "TEI / Qwen3",
                        "warn",
                        format!("{info_url} did not report a Qwen3 model"),
                    ),
                    Err(err) => check("TEI / Qwen3", "error", format!("{info_url}: {err}")),
                },
                Ok(info) => check(
                    "TEI / Qwen3",
                    "error",
                    format!("{info_url} returned {}", info.status()),
                ),
                Err(err) => check("TEI / Qwen3", "error", format!("{info_url}: {err}")),
            }
        }
        Ok(response) => check(
            "TEI / Qwen3",
            "error",
            format!("{health_url} returned {}", response.status()),
        ),
        Err(err) => check("TEI / Qwen3", "error", err.to_string()),
    }
}

fn qwen3_model_reported(body: &str) -> bool {
    body.to_ascii_lowercase().contains("qwen3")
}

async fn gemini_check() -> StackCheck {
    let mut cmd = Command::new(
        std::env::var("AXON_HEADLESS_GEMINI_CMD")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "gemini".to_string()),
    );
    cmd.arg("--version");
    match tokio::time::timeout(Duration::from_secs(4), cmd.output()).await {
        Ok(Ok(output)) if output.status.success() => {
            let home = std::env::var("AXON_HEADLESS_GEMINI_HOME")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    std::env::var("HOME")
                        .ok()
                        .map(|home| std::path::PathBuf::from(home).join(".gemini"))
                });
            match home {
                Some(path) if path.exists() => {
                    check("Gemini CLI", "ok", format!("auth home {}", path.display()))
                }
                Some(path) => check(
                    "Gemini CLI",
                    "warn",
                    format!("CLI available but {} is missing", path.display()),
                ),
                None => check("Gemini CLI", "warn", "CLI available; HOME is unset"),
            }
        }
        Ok(Ok(output)) => check(
            "Gemini CLI",
            "error",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("gemini --version failed")
                .to_string(),
        ),
        Ok(Err(err)) if err.kind() == std::io::ErrorKind::NotFound => {
            check("Gemini CLI", "error", "not found on PATH")
        }
        Ok(Err(err)) => check("Gemini CLI", "error", err.to_string()),
        Err(_) => check("Gemini CLI", "error", "timed out"),
    }
}

fn token_check() -> StackCheck {
    if axon_authz::http::configured_mcp_http_token().is_some() {
        check("MCP/API token", "ok", "AXON_HTTP_TOKEN configured")
    } else {
        check("MCP/API token", "warn", "loopback-only tokenless mode")
    }
}

fn oauth_check() -> StackCheck {
    match std::env::var("AXON_AUTH_MODE") {
        Ok(value) if value.trim().eq_ignore_ascii_case("oauth") => {
            let missing: Vec<&str> = [
                "AXON_PUBLIC_URL",
                "AXON_GOOGLE_CLIENT_ID",
                "AXON_GOOGLE_CLIENT_SECRET",
                "AXON_AUTH_ADMIN_EMAIL",
            ]
            .into_iter()
            .filter(|key| {
                std::env::var(key)
                    .ok()
                    .is_none_or(|value| value.trim().is_empty())
            })
            .collect();
            if missing.is_empty() {
                check("OAuth / lab-auth", "ok", "oauth mode configured")
            } else {
                check(
                    "OAuth / lab-auth",
                    "error",
                    format!("missing {}", missing.join(", ")),
                )
            }
        }
        _ => check("OAuth / lab-auth", "warn", "static bearer token mode"),
    }
}

fn check(label: &'static str, status: &'static str, detail: impl Into<String>) -> StackCheck {
    StackCheck {
        label,
        status,
        detail: detail.into(),
    }
}

fn url_check(
    label: &'static str,
    url: impl Into<String>,
    status: &'static str,
    detail: impl Into<String>,
) -> StackUrlCheck {
    StackUrlCheck {
        label,
        url: url.into(),
        status,
        detail: detail.into(),
    }
}

#[cfg(test)]
#[path = "panel_stack_tests.rs"]
mod tests;
