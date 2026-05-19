use crate::core::config::CommandKind;
use crate::core::config::Config;
use crate::services::setup::{self, LocalSetupInitOptions, LocalSetupMode, StackAction};
use serde::Serialize;
use serde_json::json;
use std::error::Error;
use std::time::Duration;

const PLUGIN_HOOK_TIMEOUT_SECS: u64 = 360;

const USAGE_LINES: &[&str] = &[
    "axon setup",
    "axon setup init [--auth-mode bearer|oauth] [--mcp-host HOST] [--mcp-port PORT]",
    "axon preflight",
    "axon smoke",
    "axon stack up|down|restart|rebuild",
    "axon setup plugin-hook",
    "axon setup plugin-hook --no-setup",
];

pub async fn run_setup(cfg: &Config) -> Result<(), Box<dyn Error>> {
    match cfg.command {
        CommandKind::Preflight => {
            return run_local_setup_command(cfg, LocalSetupMode::Preflight).await;
        }
        CommandKind::Smoke => return run_local_setup_command(cfg, LocalSetupMode::Smoke).await,
        CommandKind::Stack => return run_stack_command(cfg).await,
        _ => {}
    }

    match cfg.positional.first().map(String::as_str) {
        None => run_local_setup_command(cfg, LocalSetupMode::Setup).await,
        Some("plugin-hook" | "hook") => run_plugin_hook_setup_command(cfg).await,
        Some("init") => run_setup_init_command(cfg).await,
        Some("preflight" | "check") => {
            run_local_setup_command(cfg, LocalSetupMode::Preflight).await
        }
        Some("targets") => run_targets_command(cfg),
        _ => print_usage(cfg),
    }
}

async fn run_setup_init_command(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let options = parse_init_options(&cfg.positional[1..])?;
    let result = setup::run_local_setup_with_options(LocalSetupMode::Init, options).await?;
    print_local_setup_report(cfg, &result)?;
    fail_if_setup_failed(&result)
}

async fn run_stack_command(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let action = match cfg.positional.first().map(String::as_str) {
        Some("up") => StackAction::Up,
        Some("down") => StackAction::Down,
        Some("restart") => StackAction::Restart,
        Some("rebuild") => StackAction::Rebuild,
        _ => return print_usage(cfg),
    };
    let result = setup::run_stack_action(action).await?;
    print_local_setup_report(cfg, &result)?;
    fail_if_setup_failed(&result)
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
    let no_setup = cfg.positional.iter().any(|value| value == "--no-setup");
    let result = tokio::time::timeout(
        Duration::from_secs(PLUGIN_HOOK_TIMEOUT_SECS),
        build_plugin_hook_report(no_setup),
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

async fn build_plugin_hook_report(no_setup: bool) -> Result<PluginHookReport, Box<dyn Error>> {
    let check = setup::run_local_setup(LocalSetupMode::Preflight).await?;
    let needs_setup = check.has_errors || check.exceeded_hard_max;
    let setup = if needs_setup && !no_setup {
        Some(setup::run_local_setup(LocalSetupMode::Setup).await?)
    } else {
        None
    };
    Ok(PluginHookReport::new(check, setup, no_setup))
}

fn parse_init_options(args: &[String]) -> Result<LocalSetupInitOptions, Box<dyn Error>> {
    let mut options = LocalSetupInitOptions::default();
    for chunk in args.chunks(2) {
        let (flag, value) = match chunk {
            [flag, value] => (flag.as_str(), value.clone()),
            [flag] => return Err(format!("missing value for {flag}").into()),
            _ => unreachable!(),
        };
        match flag {
            "--mcp-host" => options.mcp_host = Some(value),
            "--mcp-port" => options.mcp_port = Some(value),
            "--auth-mode" => {
                if !matches!(value.as_str(), "bearer" | "oauth") {
                    return Err("--auth-mode must be bearer or oauth".into());
                }
                options.auth_mode = Some(value);
            }
            "--mcp-token" => options.mcp_token = Some(value),
            "--oauth-public-url" => options.oauth_public_url = Some(value),
            "--google-client-id" => options.google_client_id = Some(value),
            "--google-client-secret" => options.google_client_secret = Some(value),
            "--auth-admin-email" => options.auth_admin_email = Some(value),
            "--tavily-api-key" => options.tavily_api_key = Some(value),
            "--github-token" => options.github_token = Some(value),
            "--reddit-client-id" => options.reddit_client_id = Some(value),
            "--reddit-client-secret" => options.reddit_client_secret = Some(value),
            _ => return Err(format!("unknown setup init option {flag}").into()),
        }
    }
    Ok(options)
}

async fn run_local_setup_command(cfg: &Config, mode: LocalSetupMode) -> Result<(), Box<dyn Error>> {
    let result = setup::run_local_setup(mode).await?;
    print_local_setup_report(cfg, &result)?;
    fail_if_setup_failed(&result)
}

fn fail_if_setup_failed(report: &setup::LocalSetupReport) -> Result<(), Box<dyn Error>> {
    if report.has_errors {
        Err(format!("axon {} completed with failed phases", report.mode).into())
    } else if report.exceeded_hard_max {
        Err(format!("axon {} exceeded the hard maximum setup time", report.mode).into())
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
        if let Some(setup) = &report.setup {
            print_local_setup_report(cfg, setup)?;
        }
        println!("Plugin hook policy: {:?}", report.exit_policy);
        println!("Plugin hook ran setup: {}", report.ran_setup);
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
    ran_setup: bool,
    no_setup: bool,
    blocking_failures: Vec<String>,
    advisory_failures: Vec<String>,
    check: setup::LocalSetupReport,
    setup: Option<setup::LocalSetupReport>,
}

impl PluginHookReport {
    fn new(
        check: setup::LocalSetupReport,
        setup: Option<setup::LocalSetupReport>,
        no_setup: bool,
    ) -> Self {
        let active = setup.as_ref().unwrap_or(&check);
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
            ran_setup: setup.is_some(),
            no_setup,
            blocking_failures,
            advisory_failures,
            check,
            setup,
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

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
