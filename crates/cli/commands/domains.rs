use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::core::ui::{accent, muted, primary};
use crate::crates::services::system;
use crate::crates::services::types::{DetailedDomainsResult, Pagination};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;

pub async fn run_domains(cfg: &Config) -> Result<(), Box<dyn Error>> {
    log_info("command=domains");
    if !domains_detailed_mode() && try_fast_domains(cfg).await? {
        return Ok(());
    }

    let result = system::detailed_domains(cfg).await?;
    render_detailed_domains(cfg, result)
}

fn domains_detailed_mode() -> bool {
    env::var("AXON_DOMAINS_DETAILED")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
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
            let pairs: Vec<(String, usize)> = result
                .domains
                .into_iter()
                .map(|f| (f.domain, f.vectors))
                .collect();
            render_fast_domain_results(cfg, pairs)?;
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
    domains: Vec<(String, usize)>,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        let mut out: BTreeMap<String, usize> = BTreeMap::new();
        for (domain, vectors) in domains {
            out.insert(domain, vectors);
        }
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    println!("{}", primary("Domains"));
    for (domain, vectors) in domains {
        println!(
            "  {} {}",
            accent(&domain),
            muted(&format!("vectors={vectors}"))
        );
    }
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
        let mut out: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for row in result.domains {
            out.insert(row.domain, (row.vectors, row.urls));
        }
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("{}", primary("Domains"));
        for row in result.domains {
            println!(
                "  {} {}",
                accent(&row.domain),
                muted(&format!("urls={} vectors={}", row.urls, row.vectors))
            );
        }
    }
    Ok(())
}
