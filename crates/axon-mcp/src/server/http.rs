use super::AxonMcpServer;
use crate::auth::{
    AuthPolicy, build_auth_layer, configured_mcp_http_token, normalize_api_key_header,
    oauth_resource_url,
};
use crate::cors::cors_middleware;
use axon_core::config::Config;
use axon_services::context::ServiceContext;
use axum::{Router, body::Body, extract::State, middleware, middleware::Next, response::Response};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use std::sync::Arc;
use tokio::sync::OnceCell;

pub async fn mcp_http_router(
    cfg: Config,
    host: &str,
    port: u16,
    auth_policy: AuthPolicy,
    service_context: Arc<OnceCell<Arc<ServiceContext>>>,
) -> Result<Router, Box<dyn std::error::Error>> {
    // Wrap cfg in Arc once; share via clone of the Arc rather than cloning Config.
    let cfg_arc = Arc::new(cfg);
    AxonMcpServer::new_with_service_context_cell((*cfg_arc).clone(), Arc::clone(&service_context))
        .base_service_context()
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;

    // Clone auth_policy into the factory closure so each server instance
    // created by StreamableHttpService carries the correct policy.
    let auth_policy_for_factory = auth_policy.clone();
    let cfg_arc_for_factory = Arc::clone(&cfg_arc);
    let mcp_service: StreamableHttpService<AxonMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || {
                Ok(AxonMcpServer::new_with_service_context_cell(
                    (*cfg_arc_for_factory).clone(),
                    Arc::clone(&service_context),
                )
                .with_auth_policy(auth_policy_for_factory.clone()))
            },
            Default::default(),
            {
                // Build allowed-hosts list from bind address + configured origins,
                // mirroring the pattern used by syslog-mcp to allow external access
                // through reverse proxies (e.g. SWAG/Cloudflare).
                let mut hosts = vec![
                    "localhost".to_string(),
                    format!("localhost:{port}"),
                    "127.0.0.1".to_string(),
                    format!("127.0.0.1:{port}"),
                    "[::1]".to_string(),
                    format!("[::1]:{port}"),
                ];
                for origin in &cfg_arc.mcp_allowed_origins {
                    // Extract hostname from origin URL (e.g. "https://axon.tootie.tv" → "axon.tootie.tv")
                    if let Ok(uri) = origin.parse::<axum::http::Uri>()
                        && let Some(authority) = uri.authority()
                    {
                        let h = authority.host().to_string();
                        hosts.push(h.clone());
                        hosts.push(format!("{h}:{port}"));
                        hosts.push(format!("{h}:443"));
                    }
                }
                hosts.sort();
                hosts.dedup();
                StreamableHttpServerConfig::default()
                    .with_stateful_mode(true)
                    .with_allowed_hosts(hosts)
            },
        );

    let resource_url = oauth_resource_url(&auth_policy);

    let static_token: Option<Arc<str>> = configured_mcp_http_token().map(Arc::from);

    // Apply auth layer (or skip for LoopbackDev).
    // Prepend the x-api-key normalizer so that clients that previously used
    // `x-api-key: <token>` continue to work — the normalizer rewrites that
    // header to `Authorization: Bearer <token>` before AuthLayer sees the request.
    // lab-auth's AuthLayer contract is Bearer-only; normalisation happens at the
    // axon boundary so lab-auth stays header-agnostic.
    let mcp_router = Router::new().nest_service("/mcp", mcp_service);
    let authenticated: Router =
        if let Some(layer) = build_auth_layer(&auth_policy, static_token, resource_url) {
            mcp_router
                .layer(layer)
                .layer(middleware::from_fn(normalize_api_key_header))
        } else {
            mcp_router
        };

    // Mount the OAuth router when OAuth is active. These routes ARE the auth
    // flow and MUST be unauthenticated (no auth layer applied to them).
    //
    // Locked Decision: use lab_auth::routes::router() (full), NOT
    // bearer_only_router() — the full router gates /register on
    // enable_dynamic_registration (set to true in build_auth_policy), so DCR
    // is available for MCP clients (e.g. claude.ai).
    // bearer_only_router() excludes /register unconditionally.
    //
    // Locked Decision: OAuth router only when auth_state: Some(_).
    // Bearer-only (auth_state: None) and LoopbackDev have no OAuth routes.
    let oauth_router: Option<Router> = if let AuthPolicy::Mounted {
        auth_state: Some(ref state_arc),
    } = auth_policy
    {
        tracing::info!(
            "OAuth router mounted: /.well-known/oauth-authorization-server, \
             /.well-known/oauth-protected-resource, /jwks, /authorize, \
             /auth/google/callback, /token, /register"
        );
        Some(lab_auth::routes::router(state_arc.as_ref().clone()))
    } else {
        None
    };

    // Log auth startup status.
    match &auth_policy {
        AuthPolicy::LoopbackDev => tracing::warn!(
            host = %host,
            port,
            "axon: MCP HTTP server starting WITHOUT authentication (loopback dev mode)"
        ),
        AuthPolicy::Mounted { auth_state: None } => tracing::info!(
            host = %host,
            port,
            "axon: MCP HTTP server starting with static bearer auth"
        ),
        AuthPolicy::Mounted {
            auth_state: Some(_),
        } => tracing::info!(
            host = %host,
            port,
            "axon: MCP HTTP server starting with OAuth 2.0 + static bearer dual-mode auth"
        ),
    }

    // Build combined router. The OAuth router is Router<()> (AuthState baked
    // in via with_state inside lab_auth::routes::router). The cors layer is
    // applied at the outermost level.
    let base = match oauth_router {
        Some(oauth) => Router::new().merge(authenticated).merge(oauth),
        None => Router::new().merge(authenticated),
    };

    Ok(base.layer(middleware::from_fn_with_state(
        Arc::clone(&cfg_arc),
        mcp_http_cors_middleware,
    )))
}

async fn mcp_http_cors_middleware(
    State(cfg): State<Arc<Config>>,
    request: axum::http::Request<Body>,
    next: Next,
) -> Response {
    cors_middleware(request, next, &cfg.mcp_allowed_origins).await
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
