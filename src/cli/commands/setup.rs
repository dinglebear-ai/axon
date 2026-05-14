use crate::core::config::Config;
use crate::services::setup::{self, DeployRequest, LocalSetupMode};
use serde::Serialize;
use serde_json::json;
use std::error::Error;
use std::time::Duration;

const PLUGIN_HOOK_TIMEOUT_SECS: u64 = 360;

const USAGE_LINES: &[&str] = &[
    "axon setup",
    "axon setup plugin-hook",
    "axon setup plugin-hook --no-repair",
    "axon setup check",
    "axon setup repair",
    "axon setup repair --migrate-env",
];

pub async fn run_setup(cfg: &Config) -> Result<(), Box<dyn Error>> {
    match cfg.positional.first().map(String::as_str) {
        None => run_local_setup_command(cfg, LocalSetupMode::FirstRun).await,
        Some("plugin-hook" | "hook") => run_plugin_hook_setup_command(cfg).await,
        Some("check") => run_local_setup_command(cfg, LocalSetupMode::Check).await,
        Some("repair") => {
            let mode = if cfg.positional.iter().any(|value| value == "--migrate-env") {
                LocalSetupMode::MigrateEnv
            } else {
                LocalSetupMode::Repair
            };
            run_local_setup_command(cfg, mode).await
        }
        Some("targets") => run_targets_command(cfg),
        Some("deploy") => run_deploy_command(cfg).await,
        _ => print_usage(cfg),
    }
}

fn run_targets_command(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let targets = match setup::list_ssh_targets() {
        Ok(targets) => targets,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(err) => return Err(Box::new(err)),
    };

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&targets)?);
        return Ok(());
    }

    if targets.is_empty() {
        println!("No concrete SSH targets found in ~/.ssh/config");
        return Ok(());
    }

    for target in targets {
        let host = target.host_name.as_deref().unwrap_or(&target.alias);
        let user = target.user.as_deref().unwrap_or("-");
        let port = target
            .port
            .map_or_else(|| "-".to_string(), |value| value.to_string());
        println!("{}\thost={host}\tuser={user}\tport={port}", target.alias);
    }
    Ok(())
}

async fn run_deploy_command(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let target = cfg
        .positional
        .get(1)
        .ok_or("setup deploy requires an SSH target")?;
    let public_exposure = cfg.positional.iter().any(|v| v == "--public-exposure");
    let accept_new_host_key = cfg.positional.iter().any(|v| v == "--accept-new-host-key");

    let result = setup::deploy_remote(DeployRequest {
        target: target.clone(),
        remote_dir: remote_dir_from_positional(&cfg.positional),
        public_exposure: Some(public_exposure),
        accept_new_host_key: Some(accept_new_host_key),
    })
    .await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!("Deployment target: {}", result.target);
    println!("Remote host: {}", result.remote_host);
    println!("Remote dir: ~/{}", result.remote_dir);
    println!("Qdrant: {}", result.qdrant_url);
    println!("TEI: {}", result.tei_url);
    println!("Chrome: {}", result.chrome_remote_url);
    println!("Runtime env: {}", result.runtime_env_path);
    if let Some(command) = result.tunnel_command {
        println!("Tunnel: {command}");
    }
    for step in result.steps {
        println!("ok\t{}\t{}", step.name, step.detail);
    }
    Ok(())
}

fn print_usage(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "usage": USAGE_LINES }))?
        );
    } else {
        println!("Usage:");
        for line in USAGE_LINES {
            println!("  {line}");
        }
    }
    Ok(())
}

async fn run_plugin_hook_setup_command(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let no_repair = cfg.positional.iter().any(|value| value == "--no-repair");
    let result = tokio::time::timeout(
        Duration::from_secs(PLUGIN_HOOK_TIMEOUT_SECS),
        build_plugin_hook_report(no_repair),
    )
    .await;

    let report = match result {
        Ok(report) => report?,
        Err(_) => {
            let timeout = PluginHookTimeoutReport {
                exit_policy: PluginHookExitPolicy::BlockingFailure,
                timed_out: true,
                timeout_seconds: PLUGIN_HOOK_TIMEOUT_SECS,
                blocking_failures: vec!["plugin-hook-timeout".to_string()],
                advisory_failures: Vec::new(),
            };
            if cfg.json_output {
                println!("{}", serde_json::to_string_pretty(&timeout)?);
            } else {
                eprintln!(
                    "axon setup plugin-hook exceeded {}s",
                    PLUGIN_HOOK_TIMEOUT_SECS
                );
            }
            return Err("axon setup plugin-hook exceeded timeout".into());
        }
    };

    print_plugin_hook_report(cfg, &report)?;
    fail_if_plugin_hook_failed(&report)
}

async fn build_plugin_hook_report(no_repair: bool) -> Result<PluginHookReport, Box<dyn Error>> {
    let check = setup::run_local_setup(LocalSetupMode::Check).await?;
    let needs_repair = check.has_errors || check.exceeded_hard_max;
    let repair = if needs_repair && !no_repair {
        Some(setup::run_local_setup(LocalSetupMode::Repair).await?)
    } else {
        None
    };
    Ok(PluginHookReport::new(check, repair, no_repair))
}

async fn run_local_setup_command(cfg: &Config, mode: LocalSetupMode) -> Result<(), Box<dyn Error>> {
    let result = setup::run_local_setup(mode).await?;
    print_local_setup_report(cfg, &result)?;
    fail_if_setup_failed(&result)
}

fn fail_if_setup_failed(report: &setup::LocalSetupReport) -> Result<(), Box<dyn Error>> {
    if report.has_errors {
        Err("axon setup completed with failed phases".into())
    } else if report.exceeded_hard_max {
        Err("axon setup exceeded the hard maximum setup time".into())
    } else {
        Ok(())
    }
}

fn fail_if_plugin_hook_failed(report: &PluginHookReport) -> Result<(), Box<dyn Error>> {
    match report.exit_policy {
        PluginHookExitPolicy::Success => Ok(()),
        PluginHookExitPolicy::AdvisoryFailure => {
            eprintln!(
                "axon setup plugin-hook: continuing after advisory setup failures; inspect setup report"
            );
            Ok(())
        }
        PluginHookExitPolicy::BlockingFailure => Err(format!(
            "axon setup plugin-hook completed with blocking failed phases: {}",
            report.blocking_failures.join(", ")
        )
        .into()),
    }
}

fn print_plugin_hook_report(cfg: &Config, report: &PluginHookReport) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(report)?);
        Ok(())
    } else {
        print_local_setup_report(cfg, &report.check)?;
        if let Some(repair) = &report.repair {
            print_local_setup_report(cfg, repair)?;
        }
        println!("Plugin hook policy: {:?}", report.exit_policy);
        println!("Plugin hook ran repair: {}", report.ran_repair);
        if !report.blocking_failures.is_empty() {
            println!(
                "Plugin hook blocking failures: {}",
                report.blocking_failures.join(", ")
            );
        }
        if !report.advisory_failures.is_empty() {
            println!(
                "Plugin hook advisory failures: {}",
                report.advisory_failures.join(", ")
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PluginHookExitPolicy {
    Success,
    AdvisoryFailure,
    BlockingFailure,
}

#[derive(Debug, Serialize)]
struct PluginHookReport {
    exit_policy: PluginHookExitPolicy,
    ran_repair: bool,
    no_repair: bool,
    blocking_failures: Vec<String>,
    advisory_failures: Vec<String>,
    check: setup::LocalSetupReport,
    repair: Option<setup::LocalSetupReport>,
}

impl PluginHookReport {
    fn new(
        check: setup::LocalSetupReport,
        repair: Option<setup::LocalSetupReport>,
        no_repair: bool,
    ) -> Self {
        let active = repair.as_ref().unwrap_or(&check);
        let blocking_failures = blocking_failures(active);
        let advisory_failures = advisory_failures(active);
        let exit_policy = if !blocking_failures.is_empty() {
            PluginHookExitPolicy::BlockingFailure
        } else if !advisory_failures.is_empty() || active.exceeded_hard_max {
            PluginHookExitPolicy::AdvisoryFailure
        } else {
            PluginHookExitPolicy::Success
        };
        Self {
            exit_policy,
            ran_repair: repair.is_some(),
            no_repair,
            blocking_failures,
            advisory_failures,
            check,
            repair,
        }
    }
}

#[derive(Debug, Serialize)]
struct PluginHookTimeoutReport {
    exit_policy: PluginHookExitPolicy,
    timed_out: bool,
    timeout_seconds: u64,
    blocking_failures: Vec<String>,
    advisory_failures: Vec<String>,
}

fn blocking_failures(report: &setup::LocalSetupReport) -> Vec<String> {
    let mut failures: Vec<String> = report
        .phases
        .iter()
        .filter(|phase| {
            matches!(phase.status, setup::LocalSetupStatus::Error) && !phase.is_hook_advisory()
        })
        .map(|phase| phase.name.to_string())
        .collect();
    if report.exceeded_hard_max {
        failures.push("setup-hard-max".to_string());
    }
    failures
}

fn advisory_failures(report: &setup::LocalSetupReport) -> Vec<String> {
    report
        .phases
        .iter()
        .filter(|phase| {
            matches!(phase.status, setup::LocalSetupStatus::Error) && phase.is_hook_advisory()
        })
        .map(|phase| phase.name.to_string())
        .collect()
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
    println!(
        "Token: AXON_MCP_HTTP_TOKEN presence is reported in setup phases; values are never printed"
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::setup::{LocalSetupPhase, LocalSetupReport, LocalSetupStatus};
    use std::path::PathBuf;

    fn report_with_phase(name: &'static str, status: LocalSetupStatus) -> LocalSetupReport {
        LocalSetupReport {
            mode: "check",
            elapsed_ms: 0,
            target_seconds: 120,
            hard_max_seconds: 300,
            met_target: true,
            exceeded_hard_max: false,
            axon_home: PathBuf::from("/tmp/axon"),
            env_path: PathBuf::from("/tmp/axon/.env"),
            config_path: PathBuf::from("/tmp/axon/config.toml"),
            compose_dir: PathBuf::from("/tmp/axon/compose"),
            web_panel_url: "http://127.0.0.1:8001".to_string(),
            mcp_url: "http://127.0.0.1:8001/mcp".to_string(),
            has_errors: matches!(status, LocalSetupStatus::Error),
            phases: vec![LocalSetupPhase {
                name,
                status,
                detail: "phase detail".to_string(),
                elapsed_ms: 0,
            }],
        }
    }

    fn report_with(status: LocalSetupStatus) -> LocalSetupReport {
        report_with_phase("test", status)
    }

    #[test]
    fn setup_failure_gate_rejects_error_reports() {
        assert!(fail_if_setup_failed(&report_with(LocalSetupStatus::Error)).is_err());
    }

    #[test]
    fn setup_failure_gate_allows_warning_reports() {
        assert!(fail_if_setup_failed(&report_with(LocalSetupStatus::Warn)).is_ok());
    }

    #[test]
    fn hook_failure_gate_rejects_blocking_setup_errors() {
        let report = PluginHookReport::new(
            report_with_phase("docker", LocalSetupStatus::Error),
            None,
            false,
        );
        assert!(fail_if_plugin_hook_failed(&report).is_err());
    }

    #[test]
    fn hook_failure_gate_allows_advisory_smoke_errors() {
        let report = PluginHookReport::new(
            report_with_phase("ask-smoke", LocalSetupStatus::Error),
            None,
            false,
        );
        assert!(fail_if_plugin_hook_failed(&report).is_ok());
    }
}
