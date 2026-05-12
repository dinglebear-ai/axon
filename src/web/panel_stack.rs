use super::server::{AppState, authorized};
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
    server_url: String,
    mcp_url: String,
    log_dir: String,
    compose_file: String,
    checks: Vec<StackCheck>,
}

#[derive(Serialize)]
struct StackCheck {
    label: &'static str,
    status: &'static str,
    detail: String,
}

pub(super) async fn stack_status(
    State((state, cfg)): State<(AppState, Arc<crate::core::config::Config>)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    let home =
        crate::core::paths::axon_home_dir().unwrap_or_else(|| std::path::PathBuf::from("~/.axon"));
    let compose_file = home.join("compose/docker-compose.yaml");
    let qdrant_url = format!("{}/readyz", cfg.qdrant_url.trim_end_matches('/'));
    let chrome_url = cfg.chrome_remote_url.clone();
    let (docker, compose, nvidia, qdrant, tei, chrome, gemini) = tokio::join!(
        command_check("Docker", "docker", ["--version"]),
        command_check("Docker Compose", "docker", ["compose", "version"]),
        command_check(
            "NVIDIA runtime",
            "nvidia-smi",
            ["--query-gpu=name", "--format=csv,noheader"],
        ),
        http_check("Qdrant", &qdrant_url),
        tei_check(&cfg.tei_url),
        async {
            if let Some(chrome_url) = chrome_url.as_deref() {
                http_check("Chrome", chrome_url).await
            } else {
                check("Chrome", "warn", "AXON_CHROME_REMOTE_URL is unset")
            }
        },
        gemini_check()
    );
    let checks = vec![
        docker,
        compose,
        nvidia,
        compose_file_check(&compose_file),
        qdrant,
        tei,
        chrome,
        gemini,
        token_check(),
        oauth_check(),
    ];

    let server_host = browser_display_host(&cfg.mcp_http_host);
    let server_url = format!("http://{}:{}", server_host, cfg.mcp_http_port);
    Json(StackResponse {
        mcp_url: format!("{server_url}/mcp"),
        server_url,
        log_dir: home.join("logs").display().to_string(),
        compose_file: compose_file.display().to_string(),
        checks,
    })
    .into_response()
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
    let mut cmd = Command::new(program);
    cmd.args(args);
    match tokio::time::timeout(Duration::from_secs(4), cmd.output()).await {
        Ok(Ok(output)) if output.status.success() => check(
            label,
            "ok",
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("available")
                .to_string(),
        ),
        Ok(Ok(output)) => check(
            label,
            "error",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("command failed")
                .to_string(),
        ),
        Ok(Err(err)) if err.kind() == std::io::ErrorKind::NotFound => {
            check(label, "error", "not found on PATH")
        }
        Ok(Err(err)) => check(label, "error", err.to_string()),
        Err(_) => check(label, "error", "timed out"),
    }
}

fn compose_file_check(path: &std::path::Path) -> StackCheck {
    if path.exists() {
        check("Compose assets", "ok", format!("found {}", path.display()))
    } else {
        check(
            "Compose assets",
            "warn",
            format!("missing {}; run axon setup repair", path.display()),
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
                Ok(info) if info.status().is_success() => {
                    let body = info.text().await.unwrap_or_default();
                    if body.contains("Qwen3-Embedding-0.6B") {
                        check("TEI / Qwen3", "ok", "healthy; Qwen3 model reported")
                    } else {
                        check(
                            "TEI / Qwen3",
                            "warn",
                            "healthy; model info did not report Qwen3",
                        )
                    }
                }
                _ => check(
                    "TEI / Qwen3",
                    "ok",
                    "health endpoint ready; model info unavailable",
                ),
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
    if crate::mcp::auth::configured_mcp_http_token().is_some() {
        check("MCP/API token", "ok", "AXON_MCP_HTTP_TOKEN configured")
    } else {
        check("MCP/API token", "warn", "loopback-only tokenless mode")
    }
}

fn oauth_check() -> StackCheck {
    match std::env::var("AXON_MCP_AUTH_MODE") {
        Ok(value) if value.trim().eq_ignore_ascii_case("oauth") => {
            let missing: Vec<&str> = [
                "AXON_MCP_PUBLIC_URL",
                "AXON_MCP_GOOGLE_CLIENT_ID",
                "AXON_MCP_GOOGLE_CLIENT_SECRET",
                "AXON_MCP_AUTH_ADMIN_EMAIL",
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
