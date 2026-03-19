//! Shared job infrastructure: pool creation, AMQP channel, job lifecycle helpers.
//!
//! ## Patterns
//! - Create [`PgPool`] once per worker via [`make_pool`]; pass as `&PgPool` everywhere.
//! - All AMQP work goes through [`open_amqp_channel`] (5 s timeout).
//! - Use [`claim_next_pending`] → [`mark_job_failed`] /
//!   `mark_job_completed` — never write raw SQL job state updates.
//! - All internal channels are bounded (`channel(256)`); never use `unbounded_channel`.

mod amqp;
pub(crate) mod heartbeat;
mod job_ops;
pub(crate) mod pool;
mod schema;
pub(crate) mod sort;
pub(crate) mod stats;
pub(crate) mod watchdog;

// Re-exported for tests (used via `use super::*` in tests.rs).
#[cfg(test)]
use anyhow::Result;
#[cfg(test)]
use chrono::Utc;
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use sqlx::postgres::PgPoolOptions;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::env;
#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::sync::LazyLock;

use lapin::options::QueueDeclareOptions;

// Re-export all public items so existing `use crate::crates::jobs::common::*` continues to work.
pub use amqp::{batch_enqueue_jobs, enqueue_job, open_amqp_channel};
pub(crate) use amqp::{open_amqp_connection_and_channel, purge_queue_safe};
pub use heartbeat::{spawn_content_aware_heartbeat, spawn_heartbeat_task};
pub use job_ops::{
    batched_cleanup_terminal_jobs, cancel_pending_or_running_job, claim_next_pending,
    claim_pending_by_id, mark_job_completed, mark_job_failed, touch_running_job,
};
pub use pool::make_pool;
pub(crate) use schema::begin_schema_migration_tx;
pub(crate) use sort::sort_rows_for_status_view;
pub use stats::{count_stale_and_pending_jobs, count_stale_and_pending_jobs_with_pool};
#[cfg(test)]
pub(crate) use watchdog::stale_watchdog_confirmed;
#[cfg(test)]
pub(crate) use watchdog::stale_watchdog_payload;
pub use watchdog::{WatchdogSweepStats, reclaim_stale_running_jobs};

fn durable_queue_options() -> QueueDeclareOptions {
    QueueDeclareOptions {
        durable: true,
        auto_delete: false,
        exclusive: false,
        nowait: false,
        passive: false,
    }
}

#[derive(Debug, Clone, Copy)]
pub enum JobTable {
    Crawl,
    Refresh,
    Extract,
    Embed,
    Ingest,
    Graph,
}

impl JobTable {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Crawl => "axon_crawl_jobs",
            Self::Refresh => "axon_refresh_jobs",
            Self::Extract => "axon_extract_jobs",
            Self::Embed => "axon_embed_jobs",
            Self::Ingest => "axon_ingest_jobs",
            Self::Graph => "axon_graph_jobs",
        }
    }
}

#[cfg(test)]
pub(crate) fn test_config(pg_url: &str) -> crate::crates::core::config::Config {
    use crate::crates::core::config::Config;
    use std::path::PathBuf;
    Config {
        pg_url: pg_url.to_string(),
        redis_url: "redis://127.0.0.1:1".to_string(),
        amqp_url: "amqp://guest:guest@127.0.0.1:1/%2f".to_string(),
        collection: "test".to_string(),
        output_dir: PathBuf::from(".cache/test-worker-jobs"),
        crawl_queue: "axon.test.crawl".to_string(),
        extract_queue: "axon.test.extract".to_string(),
        embed_queue: "axon.test.embed".to_string(),
        ingest_queue: "axon.test.ingest".to_string(),
        tei_url: "http://127.0.0.1:1".to_string(),
        qdrant_url: "http://127.0.0.1:1".to_string(),
        openai_base_url: "http://127.0.0.1:1/v1".to_string(),
        openai_api_key: "test".to_string(),
        openai_model: "test-model".to_string(),
        // Test-specific overrides from the original literal
        search_limit: 5,
        max_pages: 10,
        max_depth: 2,
        min_markdown_chars: 50,
        drop_thin_markdown: false,
        embed: false,
        batch_concurrency: 2,
        yes: true,
        crawl_concurrency_limit: Some(2),
        backfill_concurrency_limit: Some(2),
        request_timeout_ms: Some(5_000),
        fetch_retries: 0,
        retry_backoff_ms: 0,
        ask_max_context_chars: 12_000,
        ..Config::default()
    }
}

#[cfg(test)]
pub(crate) fn parse_dotenv_content(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        if key.is_empty() {
            continue;
        }
        let mut value = v.trim().to_string();
        if ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
            && value.len() >= 2
        {
            value = value[1..value.len() - 1].to_string();
        }
        map.insert(key.to_string(), value);
    }
    map
}

#[cfg(test)]
fn read_dotenv_map() -> HashMap<String, String> {
    let Ok(content) = fs::read_to_string(".env") else {
        return HashMap::new();
    };
    parse_dotenv_content(&content)
}

#[cfg(test)]
fn non_empty_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
fn non_empty_map(map: &HashMap<String, String>, name: &str) -> Option<String> {
    map.get(name).cloned().filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
pub(crate) fn resolve_test_pg_url() -> Option<String> {
    static RESOLVED_TEST_PG_URL: LazyLock<Option<String>> = LazyLock::new(|| {
        let explicit = non_empty_env("AXON_TEST_PG_URL");
        if let Some(url) = explicit {
            return Some(crate::crates::core::config::parse::normalize_local_service_url(url));
        }

        let dotenv = read_dotenv_map();
        let axon_test_pg_url = non_empty_map(&dotenv, "AXON_TEST_PG_URL");
        if let Some(url) = axon_test_pg_url {
            return Some(crate::crates::core::config::parse::normalize_local_service_url(url));
        }

        // Do not fall through to AXON_PG_URL — that is the production database.
        // If AXON_TEST_PG_URL is not set, tests that require Postgres are skipped.
        None
    });

    RESOLVED_TEST_PG_URL.clone()
}

#[cfg(test)]
pub(crate) fn resolve_test_amqp_url() -> Option<String> {
    let explicit = non_empty_env("AXON_TEST_AMQP_URL");
    if let Some(url) = explicit {
        return Some(crate::crates::core::config::parse::normalize_local_service_url(url));
    }
    let dotenv = read_dotenv_map();
    if let Some(url) = non_empty_map(&dotenv, "AXON_TEST_AMQP_URL") {
        return Some(crate::crates::core::config::parse::normalize_local_service_url(url));
    }
    // Do not fall through to AXON_AMQP_URL — that is the production broker.
    None
}

#[cfg(test)]
pub(crate) fn resolve_test_redis_url() -> Option<String> {
    let explicit = non_empty_env("AXON_TEST_REDIS_URL");
    if let Some(url) = explicit {
        return Some(crate::crates::core::config::parse::normalize_local_service_url(url));
    }
    let dotenv = read_dotenv_map();
    if let Some(url) = non_empty_map(&dotenv, "AXON_TEST_REDIS_URL") {
        return Some(crate::crates::core::config::parse::normalize_local_service_url(url));
    }
    None
}

#[cfg(test)]
pub(crate) fn resolve_test_qdrant_url() -> Option<String> {
    let explicit = non_empty_env("AXON_TEST_QDRANT_URL");
    if let Some(url) = explicit {
        return Some(crate::crates::core::config::parse::normalize_local_service_url(url));
    }
    let dotenv = read_dotenv_map();
    if let Some(url) = non_empty_map(&dotenv, "AXON_TEST_QDRANT_URL") {
        return Some(crate::crates::core::config::parse::normalize_local_service_url(url));
    }
    None
}

#[cfg(test)]
mod tests;
