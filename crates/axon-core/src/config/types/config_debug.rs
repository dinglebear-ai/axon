//! `fmt::Debug` for [`Config`], split out of `config_impls.rs` to keep that file
//! under the repository's 500-line cap. Secret-bearing fields are redacted.
//!
//! Field list mirrors the `Config` struct; keep in sync when adding fields.

use super::config::Config;
use std::fmt;

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("command", &self.command)
            .field("start_url", &self.start_url)
            .field("positional", &self.positional)
            .field("urls_csv", &self.urls_csv)
            .field("url_glob", &self.url_glob)
            .field("query", &self.query)
            .field("search_limit", &self.search_limit)
            .field("retrieve_max_points", &self.retrieve_max_points)
            .field("train_best_rank", &self.train_best_rank)
            .field("train_notes", &self.train_notes)
            .field("max_pages", &self.max_pages)
            .field("max_depth", &self.max_depth)
            .field("include_subdomains", &self.include_subdomains)
            .field("exclude_path_prefix", &self.exclude_path_prefix)
            .field("output_dir", &self.output_dir)
            .field("output_path", &self.output_path)
            .field("warc_output", &self.warc_output)
            .field("automation_script", &self.automation_script)
            .field("render_mode", &self.render_mode)
            .field("chrome_remote_url", &self.chrome_remote_url)
            .field("chrome_proxy", &self.chrome_proxy)
            .field("user_agent", &self.user_agent)
            .field("chrome_user_agent", &self.chrome_user_agent)
            .field(
                "chrome_bootstrap_timeout_ms",
                &self.chrome_bootstrap_timeout_ms,
            )
            .field("chrome_bootstrap_retries", &self.chrome_bootstrap_retries)
            .field(
                "chrome_remote_local_policy",
                &self.chrome_remote_local_policy,
            )
            .field("respect_robots", &self.respect_robots)
            .field("min_markdown_chars", &self.min_markdown_chars)
            .field("drop_thin_markdown", &self.drop_thin_markdown)
            .field("discover_sitemaps", &self.discover_sitemaps)
            .field("sitemap_since_days", &self.sitemap_since_days)
            .field("endpoints_include_bundles", &self.endpoints_include_bundles)
            .field(
                "endpoints_first_party_only",
                &self.endpoints_first_party_only,
            )
            .field("endpoints_unique_only", &self.endpoints_unique_only)
            .field("endpoints_max_scripts", &self.endpoints_max_scripts)
            .field("endpoints_max_scan_bytes", &self.endpoints_max_scan_bytes)
            .field("endpoints_verify", &self.endpoints_verify)
            .field("endpoints_capture_network", &self.endpoints_capture_network)
            .field("endpoints_probe_rpc", &self.endpoints_probe_rpc)
            .field(
                "endpoints_probe_rpc_subdomains",
                &self.endpoints_probe_rpc_subdomains,
            )
            .field("max_sitemaps", &self.max_sitemaps)
            .field("discover_llms_txt", &self.discover_llms_txt)
            .field("max_llms_txt_urls", &self.max_llms_txt_urls)
            .field("cache", &self.cache)
            .field("cache_http_only", &self.cache_http_only)
            .field("etag_conditional", &self.etag_conditional)
            .field("path_budgets", &self.path_budgets)
            .field("format", &self.format)
            .field("collection", &self.collection)
            .field("embed", &self.embed)
            .field("mcp_embed_allowed_roots", &self.mcp_embed_allowed_roots)
            .field("mcp_embed_max_local_bytes", &self.mcp_embed_max_local_bytes)
            .field("mcp_embed_max_local_depth", &self.mcp_embed_max_local_depth)
            .field(
                "mcp_embed_max_local_entries",
                &self.mcp_embed_max_local_entries,
            )
            .field("batch_concurrency", &self.batch_concurrency)
            .field("wait", &self.wait)
            .field("sqlite_path", &self.sqlite_path)
            .field("yes", &self.yes)
            .field("reset_stores", &self.reset_stores)
            .field("reset_dry_run", &self.reset_dry_run)
            .field("performance_profile", &self.performance_profile)
            .field("crawl_concurrency_limit", &self.crawl_concurrency_limit)
            .field(
                "backfill_concurrency_limit",
                &self.backfill_concurrency_limit,
            )
            .field("adaptive_concurrency", &self.adaptive_concurrency)
            .field("sitemap_only", &self.sitemap_only)
            .field("delay_ms", &self.delay_ms)
            .field("request_timeout_ms", &self.request_timeout_ms)
            .field("scrape_batch_timeout_secs", &self.scrape_batch_timeout_secs)
            .field("fetch_retries", &self.fetch_retries)
            .field("retry_backoff_ms", &self.retry_backoff_ms)
            .field("sessions_claude", &self.sessions_claude)
            .field("sessions_codex", &self.sessions_codex)
            .field("sessions_gemini", &self.sessions_gemini)
            .field("sessions_project", &self.sessions_project)
            .field("github_token", &"[REDACTED]")
            .field("gitlab_token", &"[REDACTED]")
            .field("gitea_token", &"[REDACTED]")
            .field("github_include_source", &self.github_include_source)
            .field("github_max_issues", &self.github_max_issues)
            .field("github_max_prs", &self.github_max_prs)
            .field("reddit_client_id", &"[REDACTED]")
            .field("reddit_client_secret", &"[REDACTED]")
            .field("reddit_sort", &self.reddit_sort)
            .field("reddit_time", &self.reddit_time)
            .field("reddit_max_posts", &self.reddit_max_posts)
            .field("reddit_min_score", &self.reddit_min_score)
            .field("reddit_depth", &self.reddit_depth)
            .field("reddit_scrape_links", &self.reddit_scrape_links)
            .field("tei_url", &self.tei_url)
            .field("qdrant_url", &self.qdrant_url)
            .field("llm_backend", &self.llm_backend)
            .field("headless_gemini_model", &self.headless_gemini_model)
            .field(
                "headless_gemini_chat_model",
                &self.headless_gemini_chat_model,
            )
            .field("headless_gemini_cmd", &self.headless_gemini_cmd)
            .field("headless_gemini_home", &self.headless_gemini_home)
            .field("codex_cmd", &self.codex_cmd)
            .field("codex_home", &self.codex_home)
            .field("codex_model", &self.codex_model)
            .field(
                "codex_completion_concurrency",
                &self.codex_completion_concurrency,
            )
            .field("codex_load_user_config", &self.codex_load_user_config)
            .field("openai_base_url", &self.openai_base_url)
            .field("openai_api_key", &"[REDACTED]")
            .field("openai_model", &self.openai_model)
            .field("synthesis_high_context", &self.synthesis_high_context)
            .field("openai_chat_model", &self.openai_chat_model)
            .field(
                "llm_completion_concurrency",
                &self.llm_completion_concurrency,
            )
            .field(
                "llm_completion_timeout_secs",
                &self.llm_completion_timeout_secs,
            )
            .field("codex_pool_idle_ttl_secs", &self.codex_pool_idle_ttl_secs)
            .field("tavily_api_key", &"[REDACTED]")
            .field("mcp_allowed_origins", &self.mcp_allowed_origins)
            .field("ask_diagnostics", &self.ask_diagnostics)
            .field("ask_explain", &self.ask_explain)
            .field("ask_stream", &self.ask_stream)
            .field("ask_follow_up", &self.ask_follow_up)
            .field("ask_session", &self.ask_session)
            .field(
                "ask_follow_up_context",
                &self.ask_follow_up_context.as_ref().map(|_| "[REDACTED]"),
            )
            .field("ask_reset_session", &self.ask_reset_session)
            .field("ask_new_session", &self.ask_new_session)
            .field("ask_list_sessions", &self.ask_list_sessions)
            .field("evaluate_responses_mode", &self.evaluate_responses_mode)
            .field("ask_max_context_chars", &self.ask_max_context_chars)
            .field("ask_candidate_limit", &self.ask_candidate_limit)
            .field("ask_chunk_limit", &self.ask_chunk_limit)
            .field("ask_full_docs", &self.ask_full_docs)
            .field("ask_full_docs_explicit", &self.ask_full_docs_explicit)
            .field("ask_backfill_chunks", &self.ask_backfill_chunks)
            .field("ask_doc_fetch_concurrency", &self.ask_doc_fetch_concurrency)
            .field("ask_doc_chunk_limit", &self.ask_doc_chunk_limit)
            .field("ask_min_relevance_score", &self.ask_min_relevance_score)
            .field("ask_authoritative_domains", &self.ask_authoritative_domains)
            .field("ask_authoritative_boost", &self.ask_authoritative_boost)
            .field(
                "ask_min_citations_nontrivial",
                &self.ask_min_citations_nontrivial,
            )
            .field("hybrid_search_enabled", &self.hybrid_search_enabled)
            .field("hybrid_search_candidates", &self.hybrid_search_candidates)
            .field("ask_hybrid_candidates", &self.ask_hybrid_candidates)
            .field("tei_max_retries", &self.tei_max_retries)
            .field("tei_request_timeout_ms", &self.tei_request_timeout_ms)
            .field("tei_max_client_batch_size", &self.tei_max_client_batch_size)
            .field("embed_tei_max_concurrent", &self.embed_tei_max_concurrent)
            .field(
                "embed_tei_max_in_flight_inputs",
                &self.embed_tei_max_in_flight_inputs,
            )
            .field(
                "embed_tei_retry_backoff_ms",
                &self.embed_tei_retry_backoff_ms,
            )
            .field(
                "embed_tei_cooldown_after_failures",
                &self.embed_tei_cooldown_after_failures,
            )
            .field("embed_tei_cooldown_secs", &self.embed_tei_cooldown_secs)
            .field(
                "embed_tei_interactive_reserved_requests",
                &self.embed_tei_interactive_reserved_requests,
            )
            .field(
                "embed_tei_background_max_concurrent_requests",
                &self.embed_tei_background_max_concurrent_requests,
            )
            .field(
                "embed_tei_maintenance_max_concurrent_requests",
                &self.embed_tei_maintenance_max_concurrent_requests,
            )
            .field(
                "embed_tei_query_instruction_enabled",
                &self.embed_tei_query_instruction_enabled,
            )
            .field("embed_pool_max_inputs", &self.embed_pool_max_inputs)
            .field("embed_prep_concurrency", &self.embed_prep_concurrency)
            .field("embed_max_chunks_per_doc", &self.embed_max_chunks_per_doc)
            .field(
                "embed_max_source_chunks_per_doc",
                &self.embed_max_source_chunks_per_doc,
            )
            .field("embed_dedupe_exact_chunks", &self.embed_dedupe_exact_chunks)
            .field("openai_embed_model", &self.openai_embed_model)
            .field(
                "openai_embed_max_client_batch_size",
                &self.openai_embed_max_client_batch_size,
            )
            .field(
                "openai_embed_max_concurrent",
                &self.openai_embed_max_concurrent,
            )
            .field(
                "openai_embed_max_in_flight_inputs",
                &self.openai_embed_max_in_flight_inputs,
            )
            .field(
                "openai_embed_pool_max_inputs",
                &self.openai_embed_pool_max_inputs,
            )
            .field("ingest_lanes", &self.ingest_lanes)
            .field("embed_lanes", &self.embed_lanes)
            .field("embed_doc_timeout_secs", &self.embed_doc_timeout_secs)
            .field("queue_summary_secs", &self.queue_summary_secs)
            .field("qdrant_point_buffer", &self.qdrant_point_buffer)
            .field("max_pending_crawl_jobs", &self.max_pending_crawl_jobs)
            .field("max_pending_embed_jobs", &self.max_pending_embed_jobs)
            .field("max_pending_extract_jobs", &self.max_pending_extract_jobs)
            .field("max_pending_ingest_jobs", &self.max_pending_ingest_jobs)
            .field("hnsw_ef_search", &self.hnsw_ef_search)
            .field("hnsw_ef_search_legacy", &self.hnsw_ef_search_legacy)
            .field("evaluate_retrieval_ab", &self.evaluate_retrieval_ab)
            .field("cron_every_seconds", &self.cron_every_seconds)
            .field("cron_max_runs", &self.cron_max_runs)
            .field(
                "watchdog_stale_timeout_secs",
                &self.watchdog_stale_timeout_secs,
            )
            .field("watchdog_confirm_secs", &self.watchdog_confirm_secs)
            .field("json_output", &self.json_output)
            .field("reclaimed_status_only", &self.reclaimed_status_only)
            .field("active_status_only", &self.active_status_only)
            .field("recent_status_only", &self.recent_status_only)
            .field("normalize", &self.normalize)
            .field(
                "chrome_network_idle_timeout_secs",
                &self.chrome_network_idle_timeout_secs,
            )
            .field("auto_switch_thin_ratio", &self.auto_switch_thin_ratio)
            .field("auto_switch_min_pages", &self.auto_switch_min_pages)
            .field(
                "crawl_broadcast_buffer_min",
                &self.crawl_broadcast_buffer_min,
            )
            .field(
                "crawl_broadcast_buffer_max",
                &self.crawl_broadcast_buffer_max,
            )
            .field("url_whitelist", &self.url_whitelist)
            .field("block_assets", &self.block_assets)
            .field("max_page_bytes", &self.max_page_bytes)
            .field("redirect_policy_strict", &self.redirect_policy_strict)
            .field("chrome_wait_for_selector", &self.chrome_wait_for_selector)
            .field("root_selector", &self.root_selector)
            .field("exclude_selector", &self.exclude_selector)
            .field("chrome_screenshot", &self.chrome_screenshot)
            .field("research_depth", &self.research_depth)
            .field("search_time_range", &self.search_time_range)
            .field("since", &self.since)
            .field("before", &self.before)
            .field("seed_url", &self.seed_url)
            .field("bypass_csp", &self.bypass_csp)
            .field("accept_invalid_certs", &self.accept_invalid_certs)
            .field("screenshot_full_page", &self.screenshot_full_page)
            .field("viewport_width", &self.viewport_width)
            .field("viewport_height", &self.viewport_height)
            .field("mcp_transport", &self.mcp_transport)
            .field("mcp_http_host", &self.mcp_http_host)
            .field("mcp_http_port", &self.mcp_http_port)
            .field(
                "custom_headers",
                &self
                    .custom_headers
                    .iter()
                    .map(|h| match h.split_once(": ") {
                        Some((name, _)) => format!("{name}: [REDACTED]"),
                        None => "[MALFORMED]".to_string(),
                    })
                    .collect::<Vec<_>>(),
            )
            .field("quiet", &self.quiet)
            .field("job_wait_timeout_secs", &self.job_wait_timeout_secs)
            .field("doctor_diagnose", &self.doctor_diagnose)
            .finish()
    }
}
