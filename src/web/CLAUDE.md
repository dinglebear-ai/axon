# src/web — HTTP Server (`axon serve`)
Last Modified: 2026-05-23

Axum-based HTTP server that backs `axon serve`. Hosts the Aurora-styled control panel, the `/v1/*` REST surface, OpenAPI docs, and the first-run setup flow. Shares the same `ServiceContext` as the CLI and MCP server — every route is a thin adapter over `src/services`.

## Module Layout

```text
web/
├── server.rs                 # entrypoint: router() + PanelRuntimeState re-export
├── server/
│   ├── routing.rs            # route tree, auth scoping (read vs write), loopback guard
│   ├── handlers.rs           # handler module index
│   ├── handlers/
│   │   ├── admin.rs          # dedupe + watch CRUD
│   │   ├── artifacts.rs      # /v1/artifacts/{*path} — safe file serving from output_dir
│   │   ├── ask.rs            # /v1/ask (RAG synthesis)
│   │   ├── async_jobs.rs     # nested routers for /v1/{crawl,embed,extract,ingest}
│   │   ├── auth.rs           # panel_state, login
│   │   ├── config.rs         # /api/panel/{config,env,status,doctor,command,ops}
│   │   ├── discovery.rs      # /v1/{sources,domains,stats,status,doctor}
│   │   ├── exploration.rs    # /v1/{map,endpoints,scrape,summarize,search,research}
│   │   ├── jobs.rs           # job query helpers shared by async_jobs
│   │   ├── rag.rs            # /v1/{query,retrieve,evaluate,suggest}
│   │   ├── rest.rs           # legacy/shared REST glue
│   │   └── setup.rs          # /api/panel/setup/targets
│   ├── error.rs              # HttpError — uniform JSON error envelope
│   ├── openapi.rs            # /docs router (Swagger/Scalar)
│   ├── openapi_jobs.rs       # OpenAPI schemas for job routes
│   ├── state.rs              # AppState + PanelRuntimeState (panel password, setup flag)
│   ├── types.rs              # request/response DTOs, body limits
│   └── utils.rs              # `authorized()` cookie/bearer check + helpers
├── auth.rs                   # panel auth token: load-or-generate ~/.axon/panel-password
├── health.rs                 # /healthz, /readyz
├── panel_first_run.rs        # /api/panel/first-run/{crawl,ask} — onboarding actions
├── panel_stack.rs            # /api/panel/stack — runtime mode + URL/health probes
├── security.rs               # HostAllowlist + host_validation_middleware (DNS rebinding guard)
├── static_assets.rs          # embedded panel SPA fallback (rust-embed)
└── *_tests.rs                # sidecar tests per ENFORCED convention
```

The whole module is wired in `src/web.rs` (the root). `src/web.rs` uses `#[path]` attributes to keep the sibling-file layout without nesting into a `web/` directory at the module-declaration level.

## Public Surface

Only two things leave the module:

```rust
pub(crate) use server::{PanelRuntimeState, router};
```

Callers (`src/cli/commands/serve.rs`) construct `PanelRuntimeState::initialize(host, port)` once, then pass it plus the shared `ServiceContext` to `router(cfg, panel, service_context, auth_policy)`.

## Route Tree

| Path prefix | Auth scope | Notes |
|---|---|---|
| `/healthz`, `/readyz` | none | Always public |
| `/docs/*` | none | Swagger/Scalar OpenAPI docs |
| `/api/panel/*` | cookie session (panel password) | First-run, config, stack, command runner |
| `/v1/capabilities`, `/v1/sources`, `/v1/domains`, `/v1/stats`, `/v1/status`, `/v1/doctor`, `/v1/query`, `/v1/retrieve`, `/v1/map`, `/v1/artifacts/{*path}` | `axon:read` | Read-only; safe for read-token clients |
| `/v1/ask`, `/v1/evaluate`, `/v1/suggest`, `/v1/scrape`, `/v1/summarize`, `/v1/search`, `/v1/research`, `/v1/endpoints`, `/v1/dedupe`, `/v1/watch*`, `/v1/{crawl,embed,extract,ingest}/*` | `axon:write` | Active network/destructive ops |
| `/v1/actions`, `/v1/migrate` | — | Stub `404` (removed from REST; use direct routes / CLI) |

Bodies are capped: `ASK_BODY_LIMIT` for `/v1/ask`, `128 KiB` for all other REST routes (`rest_body_limit` in `routing.rs`).

## Critical Patterns

### Services-First Contract
Every handler takes `State<(AppState, Arc<Config>)>` (or extension equivalent) and **delegates to `ServiceContext` methods**. No handler reads from Qdrant, spawns workers, or formats JSON beyond the wire shape — the service layer owns all of that. If you find yourself reaching for `qdrant_client` or `sqlx` inside a handler, the work belongs in `src/services/`.

### Auth Scope Enforcement
`protect_routes()` in `routing.rs` wraps each subrouter with `build_auth_layer()` (lab-auth) plus a scope check (`require_read_scope` or `require_write_scope`). If `build_auth_layer()` returns `None` (loopback dev, no token configured), the router falls back to `block_loopback_destructive_request` — a hard 401 on POST/DELETE for crawl/embed/extract/ingest/dedupe/watch. **Never bypass this guard** by adding a write route outside the `write_routes` subrouter; the loopback fallback only sees routes wrapped through `protect_routes(..., ScopeRequirement::Write)`.

### Host Allowlist (DNS Rebinding)
`HostAllowlist` in `security.rs` builds an allowed `Host:` set from the bind address, `127.0.0.1`/`localhost`/`[::1]`, plus `AXON_MCP_ALLOWED_ORIGINS`. `host_validation_middleware` 403s anything else. This is the primary DNS-rebinding defense for the panel; do not relax it without a paired CORS plan.

### Panel Password
`auth::init_panel_password()` reads `~/.axon/panel-password` or generates a 32-byte URL-safe token on first launch (printed to stderr once, file mode `0o600` with `O_NOFOLLOW`). Verification uses `subtle::ConstantTimeEq` — never `==` on the token. Cookie session validation lives in `server/utils.rs::authorized`.

### PanelRuntimeState
Carries the password + `setup_required` flag + resolved config path. Built once at server bootstrap; cloned cheaply (everything inside is `Arc`/`Copy`). `setup_required = true` when `ensure_user_config()` created `~/.axon/config.toml` on this run — the SPA uses it to route into the first-run flow.

### Error Envelope
All REST routes return `HttpError` (in `server/error.rs`) on failure — a consistent JSON shape with `status`, `code`, and `message`. Don't return ad-hoc `(StatusCode, String)` tuples from handlers; use `HttpError::new(...)` so OpenAPI schemas stay accurate.

### Test Sidecar Convention
Web follows the project-wide ENFORCED `_tests.rs` sidecar rule. `server.rs` declares three test sidecars (`server_test_support_tests.rs`, `server_dedupe_tests.rs`, `server_tests.rs`) with one `#[path]` per original `mod` block. Mirror this when adding new test modules — never collapse into a single `mod tests`.

## Adding a New REST Route

1. Pick the handler module that matches the surface (`discovery.rs` for read metadata, `rag.rs` for RAG queries, `exploration.rs` for fetch/search, `async_jobs.rs` for new job kinds, `admin.rs` for write-only ops).
2. Add the handler `async fn` calling the relevant `ServiceContext` method. Map errors via `HttpError::from(...)`.
3. Register the route in `routing.rs` inside the correct `read_routes` or `write_routes` subrouter — **the scope is determined by membership, not by an attribute**.
4. Update `openapi.rs` / `openapi_jobs.rs` with the schema so `/docs` stays accurate.
5. If the route accepts arbitrary user input, confirm the default 128 KiB body cap is sufficient; otherwise layer a per-route `DefaultBodyLimit::max(...)` like `/v1/ask` does.
6. Add a sidecar `_tests.rs` exercising both the happy path and an unauthorized request.

## Testing

```bash
cargo test --lib web              # all web sidecars
cargo test --lib server_tests     # routing + auth integration
cargo test --lib security         # HostAllowlist behavior
cargo test --lib panel_first_run  # onboarding handlers
```

`server_test_support_tests.rs` builds an in-memory router with a `ServiceContext::new(cfg)` (enqueue-only) for fast handler tests; use it as the template for new route tests rather than spinning up real workers.
