use axon_core::config::Config;
use axon_core::logging::{log_info, log_warn};
use axon_core::ui::{accent, muted, primary, print_aurora_table};
use axon_services::system;
use axon_services::types::{DetailedDomainsResult, Pagination};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;

pub async fn run_domains(cfg: &Config) -> Result<(), Box<dyn Error>> {
    log_info("command=domains");
    if let Some(domain) = cfg.domains_domain.as_deref() {
        return run_domain_check(cfg, domain).await;
    }

    if !domains_detailed_mode() && try_fast_domains(cfg).await? {
        return Ok(());
    }

    let result = system::detailed_domains(cfg).await?;
    render_detailed_domains(cfg, result)
}

async fn run_domain_check(cfg: &Config, domain: &str) -> Result<(), Box<dyn Error>> {
    let result = system::domain_indexed(cfg, domain).await?;
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!("{}", primary("Domain"));
    println!(
        "  {} {}",
        accent(&result.domain),
        muted(if result.indexed {
            "indexed=true"
        } else {
            "indexed=false"
        })
    );
    Ok(())
}

fn domains_detailed_mode() -> bool {
    env::var("AXON_DOMAINS_DETAILED").ok().is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        )
    })
}

async fn try_fast_domains(cfg: &Config) -> Result<bool, Box<dyn Error>> {
    let facet_limit = env::var("AXON_DOMAINS_FACET_LIMIT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value >= 1)
        .unwrap_or(100_000)
        .clamp(1, 1_000_000);
    let pagination = Pagination {
        limit: facet_limit,
        offset: 0,
    };
    match system::domains(cfg, pagination).await {
        Ok(result) => {
            render_fast_domain_results(
                cfg,
                result.domains.into_iter().map(|f| (f.domain, f.vectors)),
            )?;
            Ok(true)
        }
        Err(err) => {
            log_warn(&format!(
                "sources_facet_fallback qdrant={} error={err}",
                cfg.qdrant_url
            ));
            Ok(false)
        }
    }
}

fn render_fast_domain_results(
    cfg: &Config,
    domains: impl IntoIterator<Item = (String, usize)>,
) -> Result<(), Box<dyn Error>> {
    let domains: Vec<(String, usize)> = domains.into_iter().collect();
    if cfg.json_output {
        let out: BTreeMap<String, usize> = domains.into_iter().collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    println!("{}", primary("Domains"));
    print_aurora_table(
        &["Domain", "Vectors"],
        domains
            .into_iter()
            .map(|(domain, vectors)| vec![domain, vectors.to_string()]),
    );
    println!(
        "{}",
        muted("Tip: set AXON_DOMAINS_DETAILED=1 for exact per-domain unique URL counts (slower).")
    );
    Ok(())
}

fn render_detailed_domains(
    cfg: &Config,
    result: DetailedDomainsResult,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        let out: BTreeMap<String, (usize, usize)> = result
            .domains
            .into_iter()
            .map(|row| (row.domain, (row.vectors, row.urls)))
            .collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("{}", primary("Domains"));
        print_aurora_table(
            &["Domain", "URLs", "Vectors"],
            result
                .domains
                .into_iter()
                .map(|row| vec![row.domain, row.urls.to_string(), row.vectors.to_string()]),
        );
    }
    Ok(())
}
