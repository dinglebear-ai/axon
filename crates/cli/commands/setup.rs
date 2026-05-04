use crate::crates::core::config::Config;
use crate::crates::services::setup::{self, DeployRequest};
use serde_json::json;
use std::error::Error;

pub async fn run_setup(cfg: &Config) -> Result<(), Box<dyn Error>> {
    match cfg.positional.first().map(String::as_str) {
        Some("targets") => {
            let targets = setup::list_ssh_targets().unwrap_or_default();
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
            let result = setup::deploy_remote(DeployRequest {
                target: target.clone(),
                remote_dir,
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
                for step in result.steps {
                    println!("ok\t{}\t{}", step.name, step.detail);
                }
            }
            Ok(())
        }
        _ => {
            let payload = json!({
                "usage": [
                    "axon setup targets",
                    "axon setup deploy <ssh-alias> [--remote-dir axon-deploy]"
                ]
            });
            if cfg.json_output {
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!("Usage:");
                println!("  axon setup targets");
                println!("  axon setup deploy <ssh-alias> [--remote-dir axon-deploy]");
            }
            Ok(())
        }
    }
}

fn remote_dir_from_positional(positional: &[String]) -> Option<String> {
    positional
        .windows(2)
        .find_map(|window| (window[0] == "--remote-dir").then(|| window[1].clone()))
}
