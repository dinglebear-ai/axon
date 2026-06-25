use super::common::start_url_from_cfg;
use axon_core::config::Config;
use axon_core::logging::log_done;
use axon_core::ui::{Spinner, accent, muted, primary, print_option, print_phase};
use axon_services::endpoints;
use axon_services::types::{EndpointOptions, EndpointReport, McpProbeOutcome};
use std::error::Error;

pub async fn run_endpoints(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let url_owned = start_url_from_cfg(cfg);
    let url = url_owned.trim();
    if url.is_empty() {
        return Err("url is required for endpoints".into());
    }
    let options = EndpointOptions {
        include_bundles: cfg.endpoints_include_bundles,
        first_party_only: cfg.endpoints_first_party_only,
        unique_only: cfg.endpoints_unique_only,
        max_scripts: cfg.endpoints_max_scripts,
        max_scan_bytes: cfg.endpoints_max_scan_bytes,
        verify: cfg.endpoints_verify,
        capture_network: cfg.endpoints_capture_network,
        probe_rpc: cfg.endpoints_probe_rpc,
        probe_rpc_subdomains: cfg.endpoints_probe_rpc_subdomains,
    };

    if options.probe_rpc_subdomains && !options.probe_rpc {
        axon_core::logging::log_warn("--probe-rpc-subdomains has no effect without --probe-rpc");
    }

    if !cfg.json_output {
        print_phase("◐", "Discovering endpoints", url);
        println!("  {}", primary("Options:"));
        print_option("includeBundles", &options.include_bundles.to_string());
        print_option("firstPartyOnly", &options.first_party_only.to_string());
        print_option("verify", &options.verify.to_string());
        print_option("captureNetwork", &options.capture_network.to_string());
        print_option("probeRpc", &options.probe_rpc.to_string());
        print_option(
            "probeRpcSubdomains",
            &options.probe_rpc_subdomains.to_string(),
        );
        println!();
    }
    let spinner = if cfg.json_output {
        None
    } else {
        Some(Spinner::new("endpoint discovery in progress"))
    };

    let report = endpoints::discover(cfg, url, options, None)
        .await
        .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?;
    if let Some(spinner) = spinner {
        spinner.finish(&format!(
            "endpoint discovery complete (endpoints={} scripts={} bundles={})",
            report.endpoints.len(),
            report.scripts_discovered,
            report.bundles_scanned
        ));
    }

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human_report(url, &report);
    }

    log_done(&format!(
        "command=endpoints endpoints={} hosts={} bundles_scanned={} truncated={} elapsed_ms={}",
        report.endpoints.len(),
        report.hosts.len(),
        report.bundles_scanned,
        report.truncated,
        report.elapsed_ms
    ));
    Ok(())
}

fn print_human_report(url: &str, report: &EndpointReport) {
    println!("{}", primary(&format!("Endpoint Results for {url}")));
    println!(
        "{} {} endpoints, {} hosts, {} bundles scanned",
        muted("Found"),
        report.endpoints.len(),
        report.hosts.len(),
        report.bundles_scanned
    );
    for warning in &report.warnings {
        println!("{} {}", muted("Warning:"), warning);
    }
    println!();
    for endpoint in &report.endpoints {
        let verified = endpoint
            .verified
            .as_ref()
            .map(|v| {
                v.status
                    .map(|status| format!(" status={status}"))
                    .unwrap_or_else(|| " unreachable".to_string())
            })
            .unwrap_or_default();
        let rpc = endpoint
            .rpc_probe
            .as_ref()
            .and_then(|p| p.protocol)
            .map(|proto| format!(" rpc={}", proto.as_str()))
            .unwrap_or_default();
        let endpoint_url = endpoint
            .normalized_url
            .as_deref()
            .unwrap_or(endpoint.value.as_str());
        let bullet = if endpoint.first_party { "•" } else { "◦" };
        println!(
            "  {} {} {}",
            accent(bullet),
            accent(endpoint_url),
            muted(&format!(
                "({}, {}{}{})",
                endpoint.kind.as_str(),
                endpoint.source.as_str(),
                verified,
                rpc
            ))
        );
    }
    let non_confirmed: Vec<_> = report
        .mcp_candidates
        .iter()
        .filter(|c| c.outcome != McpProbeOutcome::Confirmed)
        .collect();
    if !non_confirmed.is_empty() {
        println!();
        println!("  {}", muted("MCP candidates probed (not confirmed):"));
        for c in non_confirmed {
            println!(
                "  {} {} {}",
                muted("·"),
                muted(&c.url),
                muted(&format!("({})", c.outcome.as_str()))
            );
        }
    }
}
