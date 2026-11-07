//! Typed single-page scrape results and renderer HTTP outcomes.
use super::*;

pub(crate) async fn scrape_to_result_with_timeout_policy(
    cfg: &Config,
    url: &str,
    timeout_policy: crate::web_engine::browser::BrowserTimeoutPolicy,
) -> Result<axon_api::job_dto::ScrapeResult, Box<dyn Error>> {
    scrape_to_result_inner(cfg, url, timeout_policy, false).await
}

/// Preserve confirmed missing-page responses for adapter-owned retirement.
pub(crate) async fn scrape_to_result_with_http_outcomes(
    cfg: &Config,
    url: &str,
    timeout_policy: crate::web_engine::browser::BrowserTimeoutPolicy,
) -> Result<axon_api::job_dto::ScrapeResult, Box<dyn Error>> {
    scrape_to_result_inner(cfg, url, timeout_policy, true).await
}

/// Scrape a single URL and return a typed [`axon_api::job_dto::ScrapeResult`],
/// including the format-selected `output` field (markdown/html/rawHtml/json
/// per `cfg.format`). It uses the configured render mode for single-page
/// acquisition and performs no vertical-extractor dispatch (that framework was removed with `axon-extract`; see
/// `docs/pipeline-unification/plans/2026-07-04-phase-12-old-crate-removal-final-issue-sync.md`).
pub async fn scrape_to_result(
    cfg: &Config,
    url: &str,
) -> Result<axon_api::job_dto::ScrapeResult, Box<dyn Error>> {
    scrape_to_result_with_timeout_policy(
        cfg,
        url,
        crate::web_engine::browser::BrowserTimeoutPolicy::FloorForBrowserWork,
    )
    .await
}

async fn scrape_to_result_inner(
    cfg: &Config,
    url: &str,
    timeout_policy: crate::web_engine::browser::BrowserTimeoutPolicy,
    allow_missing: bool,
) -> Result<axon_api::job_dto::ScrapeResult, Box<dyn Error>> {
    let normalized = normalize_url(url);
    validate_url_with_dns(&normalized)
        .await
        .map_err(|e| format!("invalid scrape URL {normalized}: {e}"))?;

    let website = build_scrape_website(cfg, &normalized)
        .map_err(|e| format!("failed to build scrape config for {normalized}: {e}"))?;
    let mut website = crate::web_engine::browser::configure_spider_browser(
        cfg,
        website,
        cfg.render_mode,
        timeout_policy,
    )
    .await
    .map_err(|e| format!("failed to configure browser for {normalized}: {e}"))?;
    apply_automation_scripts(cfg, &mut website).await?;
    let page = fetch_single_page(cfg, &mut website, &normalized)
        .await
        .map_err(|e| format!("fetch failed for scrape of {normalized}: {e}"))?;
    validate_url_with_dns(&page.url)
        .await
        .map_err(|e| format!("render redirect target blocked for {}: {e}", page.url))?;
    let final_url = page.url;
    let html = page.html;
    let status_code = page.status_code;
    if !usable_render_response(cfg.render_mode, status_code, &html)
        && !(allow_missing && matches!(status_code, 404 | 410))
    {
        return Err(format!("scrape failed: HTTP {} for {}", status_code, normalized).into());
    }

    let sel_cfg = build_selector_config(cfg);
    let payload = build_scrape_json(&final_url, &html, status_code, sel_cfg.as_ref());
    let output = select_output(cfg.format, &final_url, &html, status_code, sel_cfg.as_ref())?;
    let mut result = map_scrape_payload(payload)?;
    result.output = output;
    Ok(result)
}
