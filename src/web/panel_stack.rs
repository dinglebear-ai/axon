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
    runtime_mode: &'static str,
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
    let runtime_mode = StackRuntimeMode::detect();
    let checks = stack_checks(
        runtime_mode,
        &compose_file,
        &qdrant_url,
        &cfg.tei_url,
        chrome_url,
    )
    .await;

    let server_host = browser_display_host(&cfg.mcp_http_host);
    let server_url = format!("http://{}:{}", server_host, cfg.mcp_http_port);
    Json(StackResponse {
        runtime_mode: runtime_mode.as_str(),
        mcp_url: format!("{server_url}/mcp"),
        server_url,
        log_dir: home.join("logs").display().to_string(),
        compose_file: compose_file.display().to_string(),
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
        if std::env::var("AXON_IN_CONTAINER")
            .ok()
            .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
            || std::path::Path::new("/.dockerenv").exists()
        {
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
        crate::services::setup::diagnostics::check_command(program, args, Duration::from_secs(4))
            .await;
    match result.status {
        crate::services::setup::diagnostics::CommandStatus::Ok => check(label, "ok", result.detail),
        crate::services::setup::diagnostics::CommandStatus::Failed
        | crate::services::setup::diagnostics::CommandStatus::NotFound
        | crate::services::setup::diagnostics::CommandStatus::TimedOut => {
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

#[cfg(test)]
mod tests {
    use super::{
        StackResponse, StackRuntimeMode, browser_display_host, host_prerequisite_checks,
        qwen3_model_reported, tei_check,
    };
    use httpmock::Method::GET;
    use httpmock::MockServer;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn display_host_normalizes_wildcard_binds_for_browser_urls() {
        assert_eq!(browser_display_host("0.0.0.0"), "127.0.0.1");
        assert_eq!(browser_display_host("::"), "127.0.0.1");
        assert_eq!(browser_display_host("[::]"), "127.0.0.1");
        assert_eq!(browser_display_host("192.0.2.10"), "192.0.2.10");
    }

    #[test]
    fn stack_response_json_shape_includes_runtime_and_checks() {
        let response = StackResponse {
            runtime_mode: "host",
            server_url: "http://127.0.0.1:8001".to_string(),
            mcp_url: "http://127.0.0.1:8001/mcp".to_string(),
            log_dir: "/tmp/axon/logs".to_string(),
            compose_file: "/tmp/axon/compose/docker-compose.yaml".to_string(),
            checks: vec![super::check("Qdrant", "ok", "ready")],
        };

        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["runtime_mode"], "host");
        assert_eq!(value["server_url"], "http://127.0.0.1:8001");
        assert_eq!(value["mcp_url"], "http://127.0.0.1:8001/mcp");
        assert_eq!(
            value["checks"][0],
            json!({
                "label": "Qdrant",
                "status": "ok",
                "detail": "ready",
            })
        );
    }

    #[tokio::test]
    async fn container_mode_skips_host_prerequisite_failures() {
        let checks =
            host_prerequisite_checks(StackRuntimeMode::Container, Path::new("/missing")).await;

        let labels: Vec<_> = checks.iter().map(|check| check.label).collect();
        assert_eq!(
            labels,
            vec![
                "Docker",
                "Docker Compose",
                "NVIDIA runtime",
                "Compose assets",
                "Gemini CLI",
            ]
        );
        assert!(checks.iter().all(|check| check.status == "skipped"));
        assert!(
            checks
                .iter()
                .all(|check| check.detail.contains("container-served panel"))
        );
    }

    #[test]
    fn qwen3_model_detection_accepts_qwen3_variants() {
        assert!(qwen3_model_reported(
            r#"{"model_id":"Qwen/Qwen3-Embedding-0.6B"}"#
        ));
        assert!(qwen3_model_reported("text-embeddings-qwen3"));
        assert!(!qwen3_model_reported(r#"{"model_id":"BAAI/bge-large-en"}"#));
    }

    #[tokio::test]
    async fn tei_check_requires_info_qwen3_after_health() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/health");
                then.status(200).body("ok");
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/info");
                then.status(200).json_body(json!({
                    "model_id": "Qwen/Qwen3-Embedding-0.6B"
                }));
            })
            .await;

        let check = tei_check(&server.base_url()).await;
        assert_eq!(check.status, "ok");
        assert!(check.detail.contains("Qwen3 model reported"));
    }

    #[tokio::test]
    async fn tei_check_warns_when_info_lacks_qwen3() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/health");
                then.status(200).body("ok");
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/info");
                then.status(200).json_body(json!({
                    "model_id": "BAAI/bge-large-en"
                }));
            })
            .await;

        let check = tei_check(&server.base_url()).await;
        assert_eq!(check.status, "warn");
        assert!(check.detail.contains("/info"));
        assert!(check.detail.contains("Qwen3"));
    }

    #[tokio::test]
    async fn tei_check_errors_when_info_is_unavailable() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/health");
                then.status(200).body("ok");
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/info");
                then.status(503).body("warming");
            })
            .await;

        let check = tei_check(&server.base_url()).await;
        assert_eq!(check.status, "error");
        assert!(check.detail.contains("/info"));
        assert!(check.detail.contains("503"));
    }
}
