use crate::crates::core::config::Config;
use crate::crates::core::content::build_selector_config;
use crate::crates::core::http::normalize_url;
use crate::crates::core::logging::log_warn;
use crate::crates::crawl::scrape::{build_scrape_website, fetch_single_page, select_output};
use crate::crates::services::types::ScrapeResult;
use sqlx::postgres::PgPoolOptions;
use std::error::Error;

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
    Ok(ScrapeResult {
        payload,
        url,
        markdown: markdown.clone(),
        output: markdown,
    })
}

/// Scrape a single URL and return a typed [`ScrapeResult`].
///
/// Delegates to [`scrape_payload`] from the crawl layer; wraps the raw
/// JSON value into the typed service result.
pub async fn scrape(cfg: &Config, url: &str) -> Result<ScrapeResult, Box<dyn Error>> {
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
    let cfg_clone = cfg.clone();
    let url_owned = result.url.clone();
    tokio::spawn(record_scrape_seed(cfg_clone, url_owned));
    Ok(result)
}

async fn record_scrape_seed(cfg: Config, url: String) {
    if cfg.pg_url.trim().is_empty() {
        return;
    }

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

    let pool = match PgPoolOptions::new()
        .max_connections(1)
        .connect(&cfg.pg_url)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            log_warn(&format!("scrape seed db connect failed: {e}"));
            return;
        }
    };

    // Run DDL once — outside the retry loop to avoid repeated schema lock round-trips.
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
        if let Err(e) = sqlx::query(stmt).execute(&pool).await {
            log_warn(&format!("scrape seed schema setup failed: {e}"));
            return;
        }
    }

    for attempt in 1..=3 {
        let record = async {
            sqlx::query(r#"INSERT INTO axon_scrape_seeds (url, options_json) VALUES ($1, $2)"#)
                .bind(url.as_str())
                .bind(options.clone())
                .execute(&pool)
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
