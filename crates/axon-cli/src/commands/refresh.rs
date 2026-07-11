use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{
    accent, confirm_destructive, muted, primary, print_aurora_table, symbol_for_status,
};
use axon_services::context::ServiceContext;
use axon_services::refresh::{self, RefreshPlan};
use std::error::Error;

/// `axon refresh [FILTER]` — re-enqueue crawl/ingest jobs for previously indexed
/// origins. Confirms before enqueuing (respects `--yes` / non-TTY).
pub async fn run_refresh(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    log_info("command=refresh");
    let filter = cfg
        .positional
        .first()
        .map(String::as_str)
        .filter(|s| !s.is_empty());

    let plan = refresh::plan_refresh(cfg, filter, Some(service_context)).await?;
    if plan.origins.is_empty() {
        return report_empty(cfg, filter);
    }

    let (crawl, ingest, skip) = (plan.crawl_count(), plan.ingest_count(), plan.skip_count());
    if !cfg.json_output {
        render_plan(&plan);
    }

    let prompt = format!(
        "Re-enqueue {crawl} crawl + {ingest} ingest job(s)? \
         ({skip} non-re-runnable origin(s) will be skipped)"
    );
    if !confirm_destructive(cfg, &prompt)? {
        return report_aborted(cfg, crawl, ingest, skip);
    }

    let outcome = refresh::execute_refresh(cfg, service_context, &plan).await?;
    report_outcome(cfg, &outcome)?;
    // Partial failure must be visible to scripts: the per-origin failures are
    // already rendered above, so exit nonzero instead of pretending success.
    if !outcome.failures.is_empty() {
        return Err(format!(
            "refresh: {} of {} origin(s) failed to enqueue",
            outcome.failures.len(),
            plan.origins.len()
        )
        .into());
    }
    Ok(())
}

fn render_plan(plan: &RefreshPlan) {
    println!("{}", primary("Refresh plan"));
    print_aurora_table(
        &["Action", "Source", "Chunks", "Origin"],
        plan.origins.iter().map(|o| {
            vec![
                o.action.label().to_string(),
                o.source_type.clone(),
                o.chunks.to_string(),
                o.seed_url.clone(),
            ]
        }),
    );
}

fn report_empty(cfg: &Config, filter: Option<&str>) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "origins": 0, "crawl_enqueued": 0, "ingest_enqueued": 0, "skipped": 0
            }))?
        );
    } else if let Some(f) = filter {
        println!("{} no indexed origins match {f:?}", muted("·"));
    } else {
        println!("{}", muted("No indexed origins with a seed_url found."));
        println!(
            "{}",
            muted("Re-crawl or re-ingest content to populate origin markers, then run refresh.")
        );
    }
    Ok(())
}

fn report_aborted(
    cfg: &Config,
    crawl: usize,
    ingest: usize,
    skip: usize,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "aborted": true,
                "crawl_planned": crawl,
                "ingest_planned": ingest,
                "skipped": skip,
            }))?
        );
    } else {
        println!("{} refresh aborted", symbol_for_status("canceled"));
    }
    Ok(())
}

fn report_outcome(cfg: &Config, outcome: &refresh::RefreshOutcome) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        let failures: Vec<_> = outcome
            .failures
            .iter()
            .map(|(origin, error)| serde_json::json!({ "origin": origin, "error": error }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "crawl_enqueued": outcome.crawl_enqueued,
                "ingest_enqueued": outcome.ingest_enqueued,
                "skipped": outcome.skipped,
                "failures": failures,
            }))?
        );
        return Ok(());
    }

    println!();
    let skipped = if outcome.skipped > 0 {
        format!(" ({} skipped)", outcome.skipped)
    } else {
        String::new()
    };
    println!(
        "{} {} crawl + {} ingest job(s) enqueued{skipped}",
        symbol_for_status("completed"),
        accent(&outcome.crawl_enqueued.to_string()),
        accent(&outcome.ingest_enqueued.to_string()),
    );
    for (origin, error) in &outcome.failures {
        println!(
            "  {} {} — {error}",
            symbol_for_status("failed"),
            muted(origin)
        );
    }
    Ok(())
}
