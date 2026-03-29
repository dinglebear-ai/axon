use crate::crates::core::config::Config;
use crate::crates::core::content::build_selector_config;
use crate::crates::core::http::normalize_url;
use crate::crates::core::logging::log_warn;
use crate::crates::crawl::scrape::{build_scrape_website, fetch_single_page, select_output};
use crate::crates::services::events::ServiceEvent;
use crate::crates::services::types::ScrapeResult;
use std::error::Error;
use std::sync::OnceLock;
use tokio::sync::mpsc;

/// Map a raw JSON payload into a [`ScrapeResult`].
///
/// This is a pure function — no network required. Tests call it with JSON literals.
pub fn map_scrape_payload(payload: serde_json::Value) -> Result<ScrapeResult, Box<dyn Error>> {
    let url = payload
        .get("url")
        .and_then(serde_json::Value::as_str)
        .ok_or("scrape payload missing url")?
        .to_string();
    let markdown = payload
        .get("markdown")
        .and_then(serde_json::Value::as_str)
        .ok_or("scrape payload missing markdown")?
        .to_string();
    let output = markdown.clone();
    Ok(ScrapeResult {
        payload,
        url,
        markdown,
        output,
    })
}

/// Scrape a single URL and return a typed [`ScrapeResult`].
///
/// Delegates to [`scrape_payload`] from the crawl layer; wraps the raw
/// JSON value into the typed service result.
///
/// `tx` is an optional progress channel. Pass `None` when progress events are
/// not needed (CLI) or `Some(sender)` when the caller wants to observe
/// intermediate log events (web / MCP streaming paths). The `tx` parameter
/// is accepted for API consistency with other multi-step service functions
/// but is currently unused — scrape is a single network round-trip with no
/// intermediate steps to report.
#[must_use = "scrape returns a Result that should be handled"]
pub async fn scrape(
    cfg: &Config,
    url: &str,
    _tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<ScrapeResult, Box<dyn Error>> {
    let normalized = normalize_url(url);
    crate::crates::core::http::validate_url(&normalized).map_err(|e| -> Box<dyn Error> {
        format!("invalid scrape url {normalized}: {e}").into()
    })?;
    let mut website = build_scrape_website(cfg, &normalized).map_err(|e| -> Box<dyn Error> {
        format!("failed to build scrape config for {normalized}: {e}").into()
    })?;
    let page = fetch_single_page(cfg, &mut website, &normalized)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("fetch failed for {normalized}: {e}").into() })?;
    let status_code = page.status_code;
    if !(200..300).contains(&status_code) {
        return Err(format!("scrape failed: HTTP {} for {}", status_code, normalized).into());
    }

    let selector_config = build_selector_config(cfg);
    let payload = crate::crates::crawl::scrape::build_scrape_json(
        &normalized,
        &page.html,
        status_code,
        selector_config.as_ref(),
    );
    let output = select_output(
        cfg.format,
        &normalized,
        &page.html,
        status_code,
        selector_config.as_ref(),
    )?;
    let mut result = map_scrape_payload(payload)?;
    result.output = output;
    // Extract only the fields needed for telemetry — avoids cloning the entire
    // Config struct (~100 fields, 2-5KB heap) for a lightweight background INSERT.
    let pg_url = cfg.pg_url.clone();
    let options = serde_json::json!({
        "format": format!("{:?}", cfg.format).to_lowercase(),
        "render_mode": cfg.render_mode.to_string(),
        "request_timeout_ms": cfg.request_timeout_ms,
        "fetch_retries": cfg.fetch_retries,
        "retry_backoff_ms": cfg.retry_backoff_ms,
        "respect_robots": cfg.respect_robots,
        "embed": cfg.embed,
        "chrome_anti_bot": cfg.chrome_anti_bot,
        "chrome_stealth": cfg.chrome_stealth,
        "chrome_intercept": cfg.chrome_intercept,
    });
    let url_owned = result.url.clone();
    tokio::spawn(record_scrape_seed(pg_url, url_owned, options));
    Ok(result)
}

/// Guard: DDL for `axon_scrape_seeds` runs at most once per process.
static SCRAPE_SCHEMA_INIT: OnceLock<()> = OnceLock::new();

/// Record a scrape seed for telemetry. Reuses the shared telemetry pool from
/// `lib.rs` instead of creating a new PgPool per call (saves 5-50ms of TCP +
/// TLS handshake per invocation).
async fn record_scrape_seed(pg_url: String, url: String, options: serde_json::Value) {
    if pg_url.trim().is_empty() {
        return;
    }

    let pool = match crate::get_or_init_telemetry_pool(&pg_url).await {
        Ok(p) => p,
        Err(e) => {
            log_warn(&format!("scrape seed db connect failed: {e}"));
            return;
        }
    };

    // DDL guarded by OnceLock — only the first call per process issues the
    // CREATE TABLE / CREATE INDEX round-trips.
    if SCRAPE_SCHEMA_INIT.get().is_none() {
        for stmt in [
            r#"
            CREATE TABLE IF NOT EXISTS axon_scrape_seeds (
                id BIGSERIAL PRIMARY KEY,
                url TEXT NOT NULL,
                options_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
            "CREATE INDEX IF NOT EXISTS idx_axon_scrape_seeds_created_desc ON axon_scrape_seeds(created_at DESC)",
        ] {
            if let Err(e) = sqlx::query(stmt).execute(pool).await {
                log_warn(&format!("scrape seed schema setup failed: {e}"));
                return;
            }
        }
        let _ = SCRAPE_SCHEMA_INIT.set(());
    }

    for attempt in 1..=3 {
        let record = async {
            sqlx::query(r#"INSERT INTO axon_scrape_seeds (url, options_json) VALUES ($1, $2)"#)
                .bind(url.as_str())
                .bind(options.clone())
                .execute(pool)
                .await?;
            Ok::<(), sqlx::Error>(())
        };

        match tokio::time::timeout(std::time::Duration::from_secs(5), record).await {
            Ok(Ok(())) => return,
            Ok(Err(err)) => {
                log_warn(&format!(
                    "scrape seed record attempt {attempt}/3 failed: {err}"
                ));
            }
            Err(_) => {
                log_warn(&format!(
                    "scrape seed record attempt {attempt}/3 timed out after 5s"
                ));
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis((200 * attempt) as u64)).await;
    }
}
