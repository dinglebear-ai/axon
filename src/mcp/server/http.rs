use super::AxonMcpServer;
use crate::core::config::Config;
use crate::mcp::auth::{mcp_auth_middleware, mcp_http_token_is_configured};
use crate::mcp::cors::cors_middleware;
use crate::web::security::{HostAllowlist, host_validation_middleware};
use axum::{Router, body::Body, extract::State, middleware, middleware::Next, response::Response};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use std::sync::Arc;
use tokio::sync::OnceCell;

pub async fn run_http_server(
    cfg: Config,
    host: &str,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    enforce_mcp_http_startup_policy(host)?;
    let host_allowlist = HostAllowlist::new(host, port, &cfg.mcp_allowed_origins);

    let app = mcp_http_router(cfg)
        .await?
        .layer(middleware::from_fn_with_state(
            host_allowlist,
            host_validation_middleware,
        ));

    if !mcp_http_token_is_configured() {
        tracing::warn!(
            context = "mcp_http_startup",
            host = %host,
            "AXON_MCP_HTTP_TOKEN not set \u{2014} loopback MCP HTTP server is unauthenticated"
        );
    }
    tracing::info!(host = %host, port, "mcp_http: server starting");
    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    if let Err(err) = axum::serve(listener, app).await {
        tracing::error!(error = %err, "mcp_http: server exited with error");
        return Err(err.into());
    }
    tracing::info!("mcp_http: server shut down cleanly");
    Ok(())
}

pub async fn run_unified_server(
    cfg: Config,
    host: &str,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    enforce_mcp_http_startup_policy(host)?;
    let host_allowlist = HostAllowlist::new(host, port, &cfg.mcp_allowed_origins);
    let panel = Arc::new(crate::web::PanelRuntimeState::initialize(host, port)?);
    let setup_required = panel.setup_required();
    let cfg_arc = Arc::new(cfg.clone());
    let web_router = crate::web::router(Arc::clone(&cfg_arc), panel);
    let app = mcp_http_router(cfg)
        .await?
        .merge(web_router)
        .layer(middleware::from_fn_with_state(
            host_allowlist,
            host_validation_middleware,
        ));

    if !mcp_http_token_is_configured() {
        tracing::warn!(
            context = "mcp_http_startup",
            host = %host,
            "AXON_MCP_HTTP_TOKEN not set \u{2014} loopback /mcp endpoint is unauthenticated"
        );
    }
    tracing::info!(host = %host, port, "serve: unified web and mcp server starting");
    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    if setup_required {
        open_setup_browser(host, port);
    }
    if let Err(err) = axum::serve(listener, app).await {
        tracing::error!(error = %err, "serve: unified server exited with error");
        return Err(err.into());
    }
    tracing::info!("serve: unified server shut down cleanly");
    Ok(())
}

fn open_setup_browser(host: &str, port: u16) {
    let host = match host.trim() {
        "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
        "" => "127.0.0.1",
        value => value.trim_matches(['[', ']']),
    };
    let url = format!("http://{host}:{port}/");

    #[cfg(target_os = "linux")]
    let command = ("xdg-open", vec![url.as_str()]);
    #[cfg(target_os = "macos")]
    let command = ("open", vec![url.as_str()]);
    #[cfg(target_os = "windows")]
    let command = (
        "rundll32",
        vec!["url.dll,FileProtocolHandler", url.as_str()],
    );

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        let (program, args) = command;
        match std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(_) => tracing::info!(url = %url, "serve: opened setup wizard in browser"),
            Err(err) => tracing::warn!(url = %url, error = %err, "serve: failed to open browser"),
        }
    }
}

async fn mcp_http_router(cfg: Config) -> Result<Router, Box<dyn std::error::Error>> {
    let cors_cfg = Arc::new(cfg.clone());
    let cfg_arc = Arc::new(cfg);
    let service_context = Arc::new(OnceCell::new());
    AxonMcpServer::new_with_service_context_cell((*cfg_arc).clone(), Arc::clone(&service_context))
        .base_service_context()
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;

    let mcp_service: StreamableHttpService<AxonMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || {
                Ok(AxonMcpServer::new_with_service_context_cell(
                    (*cfg_arc).clone(),
                    Arc::clone(&service_context),
                ))
            },
            Default::default(),
            {
                let mut cfg = StreamableHttpServerConfig::default();
                cfg.stateful_mode = true;
                cfg
            },
        );

    Ok(Router::new()
        .nest_service("/mcp", mcp_service)
        .layer(middleware::from_fn(mcp_auth_middleware))
        .layer(middleware::from_fn_with_state(
            cors_cfg,
            mcp_http_cors_middleware,
        )))
}

fn enforce_mcp_http_startup_policy(host: &str) -> Result<(), Box<dyn std::error::Error>> {
    enforce_mcp_http_startup_policy_with_token(host, mcp_http_token_is_configured())
}

fn enforce_mcp_http_startup_policy_with_token(
    host: &str,
    token_configured: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if mcp_http_bind_is_loopback(host) || token_configured {
        return Ok(());
    }

    Err(format!(
        "refusing to start unauthenticated MCP HTTP server on non-loopback host '{host}'; \
         set AXON_MCP_HTTP_TOKEN or bind AXON_MCP_HTTP_HOST to 127.0.0.1/localhost"
    )
    .into())
}

fn mcp_http_bind_is_loopback(host: &str) -> bool {
    let host = host.trim();
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    let host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);

    host.parse::<std::net::IpAddr>()
        .map(|addr| addr.is_loopback())
        .unwrap_or(false)
}

async fn mcp_http_cors_middleware(
    State(cfg): State<Arc<Config>>,
    request: axum::http::Request<Body>,
    next: Next,
) -> Response {
    cors_middleware(request, next, &cfg.mcp_allowed_origins).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_http_bind_loopback_detection_accepts_loopback_hosts() {
        assert!(mcp_http_bind_is_loopback("127.0.0.1"));
        assert!(mcp_http_bind_is_loopback("::1"));
        assert!(mcp_http_bind_is_loopback("[::1]"));
        assert!(mcp_http_bind_is_loopback("localhost"));
    }

    #[test]
    fn mcp_http_bind_loopback_detection_rejects_wildcard_and_remote_hosts() {
        assert!(!mcp_http_bind_is_loopback("0.0.0.0"));
        assert!(!mcp_http_bind_is_loopback("::"));
        assert!(!mcp_http_bind_is_loopback("192.168.1.10"));
        assert!(!mcp_http_bind_is_loopback("axon.example.com"));
    }

    #[test]
    fn startup_policy_allows_loopback_without_token() {
        let result = enforce_mcp_http_startup_policy_with_token("127.0.0.1", false);

        assert!(result.is_ok());
    }

    #[test]
    fn startup_policy_rejects_non_loopback_without_token() {
        let err =
            enforce_mcp_http_startup_policy_with_token("0.0.0.0", false).expect_err("must reject");

        assert!(
            err.to_string()
                .contains("refusing to start unauthenticated"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn startup_policy_allows_non_loopback_with_token() {
        let result = enforce_mcp_http_startup_policy_with_token("0.0.0.0", true);

        assert!(result.is_ok());
    }
}
