/// WebSocket allowed execution modes.
/// IMPORTANT: This list MUST stay in sync with apps/web/lib/ws-protocol.ts (MODES constant).
/// When adding a mode here, also add it to the TypeScript MODES array in that file.
/// See docs/WS-PROTOCOL.md for the full protocol contract.
pub(super) const ALLOWED_MODES: &[&str] = &[
    "scrape",
    "crawl",
    "map",
    "extract",
    "search",
    "research",
    "embed",
    "debug",
    "doctor",
    "query",
    "retrieve",
    "ask",
    "evaluate",
    "suggest",
    "sources",
    "domains",
    "stats",
    "status",
    "dedupe",
    "github",
    "reddit",
    "youtube",
    "sessions",
    "screenshot",
    "mcp_refresh", // INTERNAL — not exposed in apps/web/lib/ws-protocol.ts MODES array
    "pulse_chat",
    "pulse_chat_probe", // INTERNAL — not exposed in apps/web/lib/ws-protocol.ts MODES array
];

pub(super) const ALLOWED_FLAGS: &[(&str, &str)] = &[
    ("max_pages", "--max-pages"),
    ("max_depth", "--max-depth"),
    ("limit", "--limit"),
    ("collection", "--collection"),
    ("format", "--format"),
    ("render_mode", "--render-mode"),
    ("include_subdomains", "--include-subdomains"),
    ("discover_sitemaps", "--discover-sitemaps"),
    ("sitemap_since_days", "--sitemap-since-days"),
    ("embed", "--embed"),
    ("diagnostics", "--diagnostics"),
    ("yes", "--yes"),
    ("wait", "--wait"),
    ("research_depth", "--research-depth"),
    ("search_time_range", "--search-time-range"),
    ("sort", "--sort"),
    ("time", "--time"),
    ("max_posts", "--max-posts"),
    ("min_score", "--min-score"),
    ("scrape_links", "--scrape-links"),
    ("include_source", "--include-source"),
    ("claude", "--claude"),
    ("codex", "--codex"),
    ("gemini", "--gemini"),
    ("project", "--project"),
    ("output_dir", "--output-dir"),
    ("delay_ms", "--delay-ms"),
    ("request_timeout_ms", "--request-timeout-ms"),
    ("performance_profile", "--performance-profile"),
    ("batch_concurrency", "--batch-concurrency"),
    ("depth", "--depth"),
    ("responses_mode", "--responses-mode"),
    ("agent", "--agent"),
    ("model", "--model"),
    ("session_mode", "--session-mode"),
    ("mcp_servers", "--mcp-servers"),
    ("blocked_mcp_tools", "--blocked-mcp-tools"),
    ("session_id", "--session-id"),
    ("assistant_mode", "--assistant-mode"),
    // ACP adapter capability flags — consumed by the pulse_chat direct-service path via
    // DirectParams; also forwarded as CLI args on subprocess paths that accept them.
    ("enable_fs", "--enable-fs"),
    ("enable_terminal", "--enable-terminal"),
    ("permission_timeout_secs", "--permission-timeout-secs"),
    ("adapter_timeout_secs", "--adapter-timeout-secs"),
    ("offset", "--offset"),
    ("max_points", "--max-points"),
];

/// ACP modes that hold their own concurrency permit via `ACP_SESSION_SEMAPHORE`.
///
/// Both `execute.rs::acquire_acp_permit` and `sync_mode.rs::is_acp_mode` derive
/// their checks from this constant to prevent the two sites from drifting apart.
pub(super) const ACP_MODES: &[&str] = &["pulse_chat", "pulse_chat_probe"];

/// Modes that use fire-and-forget direct service enqueue.
/// These produce job IDs and return immediately without polling.
pub(super) const ASYNC_MODES: &[&str] =
    &["crawl", "extract", "embed", "github", "reddit", "youtube"];

/// Modes that must NOT receive --json because their output format is inherently non-JSON.
///
/// "search" and "research" were previously listed here but are now routed through
/// `call_search`/`call_research` → `send_json_owned` in `dispatch.rs`, so they
/// produce structured JSON regardless.  They have been removed.
///
/// The only remaining use-case is `evaluate` in events mode, which is handled via
/// the `disable_json_for_evaluate_events` guard in `args.rs` rather than this list.
pub(super) const NO_JSON_MODES: &[&str] = &[];
