use std::io;

use crate::core::config::Config;

use super::ConfigSnapshot;

impl ConfigSnapshot {
    pub(super) fn apply_to(
        self,
        cfg: &mut Config,
        exact_options: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut snapshot = self;
        let fallback_fields = std::mem::take(&mut snapshot.process_fallback_fields);
        snapshot.apply_llm_backend(cfg)?;
        snapshot.apply_regular_fields(cfg);
        snapshot.apply_option_fields(cfg, exact_options, &fallback_fields);
        snapshot.apply_adaptive_concurrency(cfg)?;
        Ok(())
    }

    fn apply_llm_backend(
        &mut self,
        cfg: &mut Config,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(value) = self.llm_backend.take() {
            let kind = crate::core::llm::LlmBackendKind::parse(&value).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid llm_backend in config snapshot {value:?}: {err}"),
                )
            })?;
            cfg.llm_backend = kind;
        }
        Ok(())
    }

    fn apply_regular_fields(&mut self, cfg: &mut Config) {
        self.apply_regular_crawl_fields(cfg);
        self.apply_regular_source_fields(cfg);
        self.apply_regular_backend_fields(cfg);
        self.apply_regular_rag_fields(cfg);
        self.apply_regular_output_fields(cfg);
    }

    fn apply_regular_crawl_fields(&mut self, cfg: &mut Config) {
        macro_rules! set {
            ($($field:ident),+ $(,)?) => {
                $(if let Some(value) = self.$field.take() { cfg.$field = value; })+
            };
        }
        set!(
            collection,
            output_dir,
            search_limit,
            max_pages,
            max_depth,
            include_subdomains,
            exclude_path_prefix,
            render_mode,
            custom_headers,
            chrome_bootstrap_timeout_ms,
            chrome_bootstrap_retries,
            chrome_remote_local_policy,
            respect_robots,
            min_markdown_chars,
            drop_thin_markdown,
            discover_sitemaps,
            sitemap_since_days,
            map_fallback,
            max_sitemaps,
            discover_llms_txt,
            max_llms_txt_urls,
            cache,
            cache_http_only,
            format,
            embed,
            batch_concurrency,
            sitemap_only,
            delay_ms,
            fetch_retries,
            retry_backoff_ms,
        );
    }

    fn apply_regular_source_fields(&mut self, cfg: &mut Config) {
        macro_rules! set {
            ($($field:ident),+ $(,)?) => {
                $(if let Some(value) = self.$field.take() { cfg.$field = value; })+
            };
        }
        set!(
            sessions_claude,
            sessions_codex,
            sessions_gemini,
            github_include_source,
            github_max_issues,
            github_max_prs,
            reddit_sort,
            reddit_time,
            reddit_max_posts,
            reddit_min_score,
            reddit_depth,
            reddit_scrape_links,
        );
    }

    fn apply_regular_backend_fields(&mut self, cfg: &mut Config) {
        macro_rules! set {
            ($($field:ident),+ $(,)?) => {
                $(if let Some(value) = self.$field.take() { cfg.$field = value; })+
            };
        }
        set!(
            tei_url,
            qdrant_url,
            headless_gemini_model,
            headless_gemini_cmd,
            codex_model,
            codex_completion_concurrency,
            codex_load_user_config,
            openai_base_url,
            openai_model,
            llm_completion_concurrency,
            llm_completion_timeout_secs,
        );
    }

    fn apply_regular_rag_fields(&mut self, cfg: &mut Config) {
        macro_rules! set {
            ($($field:ident),+ $(,)?) => {
                $(if let Some(value) = self.$field.take() { cfg.$field = value; })+
            };
        }
        set!(
            ask_diagnostics,
            ask_max_context_chars,
            ask_candidate_limit,
            ask_chunk_limit,
            ask_full_docs,
            ask_backfill_chunks,
            ask_doc_fetch_concurrency,
            ask_doc_chunk_limit,
            ask_min_relevance_score,
            ask_authoritative_domains,
            ask_authoritative_boost,
            ask_min_citations_nontrivial,
            hybrid_search_enabled,
            evaluate_retrieval_ab,
            hybrid_search_candidates,
            ask_hybrid_candidates,
            normalize,
        );
    }

    fn apply_regular_output_fields(&mut self, cfg: &mut Config) {
        macro_rules! set {
            ($($field:ident),+ $(,)?) => {
                $(if let Some(value) = self.$field.take() { cfg.$field = value; })+
            };
        }
        set!(
            chrome_network_idle_timeout_secs,
            auto_switch_thin_ratio,
            auto_switch_min_pages,
            crawl_broadcast_buffer_min,
            crawl_broadcast_buffer_max,
            url_whitelist,
            block_assets,
            redirect_policy_strict,
            chrome_screenshot,
            bypass_csp,
            accept_invalid_certs,
            screenshot_full_page,
            viewport_width,
            viewport_height,
            quiet,
        );
    }

    fn apply_adaptive_concurrency(
        &mut self,
        cfg: &mut Config,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(value) = self.adaptive_concurrency.take() {
            cfg.adaptive_concurrency = value
                .into_config_for(cfg)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        }
        Ok(())
    }

    fn apply_option_fields(
        &mut self,
        cfg: &mut Config,
        exact_options: bool,
        fallback_fields: &[String],
    ) {
        macro_rules! set_option_exact {
            ($($field:ident),+ $(,)?) => {
                $(if exact_options && !fallback_fields.iter().any(|name| name == stringify!($field)) {
                    cfg.$field = self.$field.take();
                } else if let Some(value) = self.$field.take() {
                    cfg.$field = Some(value);
                })+
            };
        }
        set_option_exact!(
            output_path,
            warc_output,
            automation_script,
            query,
            chrome_remote_url,
            chrome_proxy,
            user_agent,
            chrome_user_agent,
            crawl_concurrency_limit,
            backfill_concurrency_limit,
            request_timeout_ms,
            headless_gemini_home,
            sessions_project,
            max_page_bytes,
            chrome_wait_for_selector,
            root_selector,
            exclude_selector,
            research_depth,
            search_time_range,
            since,
            before,
            seed_url,
        );
    }
}
