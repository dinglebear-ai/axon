use crate::core::config::Config;
use crate::services::setup::{self, DeployRequest, LocalSetupMode};
use serde_json::json;
use std::error::Error;

pub async fn run_setup(cfg: &Config) -> Result<(), Box<dyn Error>> {
    match cfg.positional.first().map(String::as_str) {
        None => {
            let result = setup::run_local_setup(LocalSetupMode::FirstRun).await?;
            print_local_setup_report(cfg, &result)?;
            Ok(())
        }
        Some("check") => {
            let result = setup::run_local_setup(LocalSetupMode::Check).await?;
            print_local_setup_report(cfg, &result)?;
            Ok(())
        }
        Some("repair") => {
            let result = setup::run_local_setup(LocalSetupMode::Repair).await?;
            print_local_setup_report(cfg, &result)?;
            Ok(())
        }
        Some("targets") => {
            let targets = match setup::list_ssh_targets() {
                Ok(targets) => targets,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                Err(err) => return Err(Box::new(err)),
            };
            if cfg.json_output {
                println!("{}", serde_json::to_string_pretty(&targets)?);
            } else if targets.is_empty() {
                println!("No concrete SSH targets found in ~/.ssh/config");
            } else {
                for target in targets {
                    let host = target.host_name.as_deref().unwrap_or(&target.alias);
                    let user = target.user.as_deref().unwrap_or("-");
                    let port = target
                        .port
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    println!("{}\thost={host}\tuser={user}\tport={port}", target.alias);
                }
            }
            Ok(())
        }
        Some("deploy") => {
            let target = cfg
                .positional
                .get(1)
                .ok_or("setup deploy requires an SSH target")?;
            let remote_dir = remote_dir_from_positional(&cfg.positional);
            let public_exposure = cfg
                .positional
                .iter()
                .any(|value| value == "--public-exposure");
            let accept_new_host_key = cfg
                .positional
                .iter()
                .any(|value| value == "--accept-new-host-key");
            let result = setup::deploy_remote(DeployRequest {
                target: target.clone(),
                remote_dir,
                public_exposure: Some(public_exposure),
                accept_new_host_key: Some(accept_new_host_key),
            })
            .await?;
            if cfg.json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Deployment target: {}", result.target);
                println!("Remote host: {}", result.remote_host);
                println!("Remote dir: ~/{}", result.remote_dir);
                println!("Qdrant: {}", result.qdrant_url);
                println!("TEI: {}", result.tei_url);
                println!("Chrome: {}", result.chrome_remote_url);
                println!("Config: {}", result.config_path);
                if let Some(command) = result.tunnel_command {
                    println!("Tunnel: {command}");
                }
                for step in result.steps {
                    println!("ok\t{}\t{}", step.name, step.detail);
                }
            }
            Ok(())
        }
        _ => {
            let payload = json!({
                "usage": [
                    "axon setup",
                    "axon setup check",
                    "axon setup repair",
                    "axon setup targets",
                    "axon setup deploy <ssh-alias> [--remote-dir axon-deploy] [--accept-new-host-key] [--public-exposure]"
                ]
            });
            if cfg.json_output {
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!("Usage:");
                println!("  axon setup");
                println!("  axon setup check");
                println!("  axon setup repair");
                println!("  axon setup targets");
                println!(
                    "  axon setup deploy <ssh-alias> [--remote-dir axon-deploy] [--accept-new-host-key] [--public-exposure]"
                );
            }
            Ok(())
        }
    }
}

fn print_local_setup_report(
    cfg: &Config,
    report: &setup::LocalSetupReport,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    println!("Axon setup mode: {}", report.mode);
    println!("Axon home: {}", report.axon_home.display());
    println!("Config: {}", report.config_path.display());
    println!("Env: {}", report.env_path.display());
    println!("Compose: {}", report.compose_dir.display());
    println!("Web panel: {}", report.web_panel_url);
    println!("MCP: {}", report.mcp_url);
    println!("Token: {}", report.token_path.display());
    println!(
        "Timing: {:.1}s (target {}s, hard max {}s)",
        report.elapsed_ms as f64 / 1000.0,
        report.target_seconds,
        report.hard_max_seconds
    );
    if report.met_target {
        println!("Timing status: met target");
    } else if report.exceeded_hard_max {
        println!("Timing status: exceeded hard maximum");
    } else {
        println!("Timing status: exceeded target");
    }
    for phase in &report.phases {
        println!(
            "{:?}\t{}\t{}ms\t{}",
            phase.status, phase.name, phase.elapsed_ms, phase.detail
        );
    }
    println!("Next diagnostic: axon doctor");
    Ok(())
}

fn remote_dir_from_positional(positional: &[String]) -> Option<String> {
    positional
        .windows(2)
        .find_map(|window| (window[0] == "--remote-dir").then(|| window[1].clone()))
}
