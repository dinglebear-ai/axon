use crate::core::config::CommandKind;
use crate::core::config::Config;
use crate::core::ui::{accent, muted, primary, print_aurora_table, symbol_for_status};
use crate::services::setup::{
    self, ComposeAction, LocalSetupInitOptions, LocalSetupMode, LocalSetupStatus,
};
use serde::Serialize;
use serde_json::json;
use std::error::Error;
use std::time::Duration;

const PLUGIN_HOOK_TIMEOUT_SECS: u64 = 360;

mod plugin_options;
pub use plugin_options::apply_plugin_options;

const USAGE_LINES: &[&str] = &[
    "axon setup",
    "axon setup init [--auth-mode bearer|oauth] [--mcp-host HOST] [--mcp-port PORT]",
    "axon preflight",
    "axon smoke",
    "axon compose up|down|restart|rebuild",
    "axon setup plugin-hook",
    "axon setup plugin-hook --no-setup",
];

pub async fn run_setup(cfg: &Config) -> Result<(), Box<dyn Error>> {
    match cfg.command {
        CommandKind::Preflight => {
            return run_local_setup_command(cfg, LocalSetupMode::Preflight).await;
        }
        CommandKind::Smoke => return run_local_setup_command(cfg, LocalSetupMode::Smoke).await,
        CommandKind::Compose => return run_compose_command(cfg).await,
        _ => {}
    }

    match cfg.positional.first().map(String::as_str) {
        None => run_local_setup_command(cfg, LocalSetupMode::Setup).await,
        Some("plugin-hook" | "hook") => run_plugin_hook_setup_command(cfg).await,
        Some("init") => run_setup_init_command(cfg).await,
        Some("preflight" | "check") => {
            run_local_setup_command(cfg, LocalSetupMode::Preflight).await
        }
        Some("install") => run_install_setup_command(cfg).await,
        Some("targets") => run_targets_command(cfg),
        _ => print_usage(cfg),
    }
}

/// Copy the running axon binary into ~/.local/bin so it is callable as a bare
/// command in the user's own terminal. Copy (not symlink) so it survives
/// `/plugin update`.
fn install_self() -> Result<std::path::PathBuf, Box<dyn Error>> {
    let exe = std::env::current_exe()?;
    let name = exe.file_name().ok_or("cannot determine binary name")?;
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    let bin_dir = std::path::PathBuf::from(home).join(".local").join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let dest = bin_dir.join(name);
    if dest == exe {
        return Ok(dest);
    }
    let tmp = bin_dir.join(format!(".{}.tmp", name.to_string_lossy()));
    std::fs::copy(&exe, &tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&tmp, &dest)?;
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d == bin_dir))
        .unwrap_or(false);
    if !on_path {
        eprintln!(
            "note: {} is not on your PATH; add:  export PATH=\"$HOME/.local/bin:$PATH\"",
            bin_dir.display()
        );
    }
    Ok(dest)
}

async fn run_install_setup_command(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let _ = cfg;
    let dest = install_self()?;
    println!("installed -> {}", dest.display());
    Ok(())
}

async fn run_setup_init_command(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let options = parse_init_options(&cfg.positional[1..])?;
    let result = setup::run_local_setup_with_options(LocalSetupMode::Init, options).await?;
    print_local_setup_report(cfg, &result)?;
    fail_if_setup_failed(&result)
}

async fn run_compose_command(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let action = match cfg.positional.first().map(String::as_str) {
        Some("up") => ComposeAction::Up,
        Some("down") => ComposeAction::Down,
        Some("restart") => ComposeAction::Restart,
        Some("rebuild") => ComposeAction::Rebuild,
        _ => return print_usage(cfg),
    };
    let result = setup::run_compose_action(action).await?;
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
        println!(
            "{}",
            muted("No concrete SSH targets found in ~/.ssh/config")
        );
        return Ok(());
    }

    println!("{}", primary("SSH Targets"));
    print_aurora_table(
        &["Alias", "Host", "User", "Port"],
        targets.iter().map(|target| {
            let host = target.host_name.as_deref().unwrap_or(&target.alias);
            let user = target.user.as_deref().unwrap_or("-");
            let port = target
                .port
                .map_or_else(|| "-".to_string(), |value| value.to_string());
            vec![
                accent(&target.alias),
                host.to_string(),
                user.to_string(),
                port,
            ]
        }),
    );
    Ok(())
}

fn print_usage(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "usage": USAGE_LINES }))?
        );
    } else {
        println!("{}", primary("Usage"));
        for line in USAGE_LINES {
            println!("  {}", muted(line));
        }
    }
    Ok(())
}

async fn run_plugin_hook_setup_command(cfg: &Config) -> Result<(), Box<dyn Error>> {
    // Belt-and-suspenders: the env-var mapping is applied early (before
    // parse_args) by `apply_plugin_options()` in `run()`. Re-applying here is
    // harmless and covers any direct caller that bypasses the early path. The
    // EARLY call is the one that matters for Config::load.
    apply_plugin_options();
    // Keep the user's terminal copy in ~/.local/bin fresh each session.
    let _ = install_self();
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
        println!(
            "{} {}",
            muted("plugin hook policy:"),
            accent(&format!("{:?}", report.exit_policy))
        );
        println!(
            "{} {}",
            muted("plugin hook ran setup:"),
            accent(&report.ran_setup.to_string())
        );
        if !report.blocking_failures.is_empty() {
            println!(
                "{} {}",
                primary("blocking failures:"),
                report.blocking_failures.join(", ")
            );
        }
        if !report.advisory_failures.is_empty() {
            println!(
                "{} {}",
                muted("advisory failures:"),
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
            matches!(phase.status, LocalSetupStatus::Error) && !phase.is_hook_advisory()
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
        .filter(|phase| matches!(phase.status, LocalSetupStatus::Error) && phase.is_hook_advisory())
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

    println!("{} {}", muted("setup mode:"), accent(report.mode));
    println!("{} {}", muted("axon home:"), report.axon_home.display());
    println!("{} {}", muted("config:"), report.config_path.display());
    println!("{} {}", muted("env:"), report.env_path.display());
    println!("{} {}", muted("compose:"), report.compose_dir.display());
    println!("{} {}", muted("web panel:"), accent(&report.web_panel_url));
    println!("{} {}", muted("mcp:"), accent(&report.mcp_url));
    println!(
        "{}",
        muted(
            "token: AXON_MCP_HTTP_TOKEN presence is reported in setup phases; values are never printed"
        )
    );
    println!(
        "{} {:.1}s {} {}s {} {}s{}",
        muted("timing:"),
        report.elapsed_ms as f64 / 1000.0,
        muted("target"),
        report.target_seconds,
        muted("hard max"),
        report.hard_max_seconds,
        if report.met_target {
            format!(" — {}", accent("met target"))
        } else if report.exceeded_hard_max {
            format!(" — {}", primary("exceeded hard maximum"))
        } else {
            format!(" — {}", muted("exceeded target"))
        }
    );
    println!();
    for phase in &report.phases {
        let slug = match phase.status {
            LocalSetupStatus::Ok => "completed",
            LocalSetupStatus::Warn => "warn",
            LocalSetupStatus::Error => "failed",
            LocalSetupStatus::Skipped => "pending",
        };
        println!(
            "  {} {} {}",
            symbol_for_status(slug),
            phase.name,
            muted(&format!("{}ms  {}", phase.elapsed_ms, phase.detail))
        );
    }
    println!();
    println!("{}", muted("next diagnostic: axon doctor"));
    Ok(())
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
