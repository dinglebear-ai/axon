//! Unified `axon serve` / `axon mcp --transport http` server bootstrap.
//!
//! Merges the MCP HTTP router (`axon_mcp`) with the web panel + REST router
//! (`axon_web`) under one Axum app. Lives here in the CLI — the only layer that
//! depends on both `mcp` and `web` — so neither of those crates depends on the
//! other (breaking the historical `mcp` ↔ `web` cycle).

use axon_core::config::Config;
use axon_mcp::auth::build_auth_policy;
use axon_mcp::server::mcp_http_router;
use axon_services::context::ServiceContext;
use axon_web::security::{HostAllowlist, host_validation_middleware};
use axum::middleware;
use std::sync::Arc;
use tokio::sync::OnceCell;

pub async fn run_unified_server(
    cfg: Config,
    host: &str,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    // Install the Prometheus recorder before any router is built so the
    // `/metrics` route and the ask-path metric macros have a live recorder.
    axon_web::metrics::install_recorder();
    let auth_policy = build_auth_policy(host, false).await?;
    let host_allowlist = HostAllowlist::new(host, port, &cfg.mcp_allowed_origins);
    let panel = Arc::new(axon_web::PanelRuntimeState::initialize(host, port)?);
    let setup_required = panel.setup_required();
    let cfg_arc = Arc::new(cfg.clone());
    let service_context = Arc::new(OnceCell::<Arc<ServiceContext>>::new());
    let eager_context = Arc::new(
        ServiceContext::new_with_workers(Arc::clone(&cfg_arc))
            .await
            .map_err(|e| -> Box<dyn std::error::Error> { e })?,
    );
    service_context
        .set(eager_context)
        .map_err(|_| "serve: failed to initialize service context")?;
    let web_router = axon_web::router(
        Arc::clone(&cfg_arc),
        panel,
        Arc::clone(
            service_context
                .get()
                .ok_or("serve: service context missing after eager initialization")?,
        ),
        auth_policy.clone(),
    );
    let app = mcp_http_router(cfg, host, port, auth_policy, service_context)
        .await?
        .merge(web_router)
        .layer(middleware::from_fn_with_state(
            host_allowlist,
            host_validation_middleware,
        ));

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
