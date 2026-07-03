use axon_core::config::CommandKind;
use axon_core::config::Config;
use axon_core::ui::{accent, muted, primary, print_aurora_table, symbol_for_status};
use axon_services::setup::{
    self, ComposeAction, LocalSetupInitOptions, LocalSetupMode, LocalSetupStatus,
    SessionWatchServiceReport,
};
use serde_json::json;
use std::error::Error;

mod plugin_options;
pub use plugin_options::apply_plugin_options;

const USAGE_LINES: &[&str] = &[
    "axon setup [--method pull|build] [--yes]",
    "axon setup init [--auth-mode bearer|oauth] [--mcp-host HOST] [--mcp-port PORT]",
    "axon preflight",
    "axon smoke",
    "axon compose up|down|restart|rebuild",
    "axon setup session-watch-service install|check|remove|status",
    "axon setup plugin-hook",
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

    if let Some(action) = cfg.setup_session_watch_action {
        return run_session_watch_service_setup_command(cfg, action).await;
    }

    match cfg.positional.first().map(String::as_str) {
        None => run_setup_wizard(cfg).await,
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

async fn run_session_watch_service_setup_command(
    cfg: &Config,
    action: setup::SessionWatchServiceAction,
) -> Result<(), Box<dyn Error>> {
    let report = setup::run_session_watch_service_setup(action).await?;
    print_session_watch_service_report(cfg, &report)?;
    if report.has_errors {
        return Err(format!("session watch service {} failed", action.as_str()).into());
    }
    Ok(())
}

/// Full setup wizard: init env/compose → start stack → install self to ~/.local/bin.
///
/// This is the default action when `axon setup` is invoked with no subcommand,
/// including when called from `install.sh` after the binary has been placed.
/// `cfg.setup_method` carries the acquisition method (`pull`/`build`/`None`).
async fn run_setup_wizard(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if let Some(method) = &cfg.setup_method
        && !cfg.json_output
    {
        println!("{} {}", muted("acquisition method:"), accent(method));
    }
    run_local_setup_command(cfg, LocalSetupMode::Setup).await?;
    match install_self() {
        Ok(dest) => {
            if cfg.json_output {
                // json output already printed by run_local_setup_command; no-op here
            } else {
                println!(
                    "{} {}",
                    muted("installed binary →"),
                    accent(&dest.display().to_string())
                );
            }
        }
        Err(err) => {
            // Non-fatal: stack is up, user just needs to copy manually.
            if cfg.json_output {
                eprintln!(
                    "{{\"install_self_warn\": {}}}",
                    serde_json::to_string(&err.to_string())?
                );
            } else {
                eprintln!("warn: self-install skipped: {err}");
            }
        }
    }
    Ok(())
}

/// Copy the running axon binary into the platform local bin dir so it is
/// callable as a bare command. Copy (not symlink) so it survives `/plugin update`.
fn install_self() -> Result<std::path::PathBuf, Box<dyn Error>> {
    let exe = std::env::current_exe()?;
    let name = exe.file_name().ok_or("cannot determine binary name")?;
    let bin_dir = local_bin_dir().ok_or("cannot determine home directory")?;
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
        #[cfg(windows)]
        eprintln!(
            "note: {} is not on your PATH; add it with:\n  [Environment]::SetEnvironmentVariable('Path', $env:Path + ';{}', 'User')",
            bin_dir.display(),
            bin_dir.display()
        );
        #[cfg(not(windows))]
        eprintln!(
            "note: {} is not on your PATH; add:  export PATH=\"$HOME/.local/bin:$PATH\"",
            bin_dir.display()
        );
    }
    Ok(dest)
}

/// Returns `$HOME/.local/bin` on Unix/macOS, `%USERPROFILE%\.local\bin` on Windows.
fn local_bin_dir() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(|h| std::path::PathBuf::from(h).join(".local").join("bin"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local").join("bin"))
    }
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

fn print_session_watch_service_report(
    cfg: &Config,
    report: &SessionWatchServiceReport,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    println!(
        "{} {}",
        primary("Session Watch Service"),
        muted(report.action.as_str())
    );
    println!("{} {}", muted("unit:"), report.unit_path.display());
    println!("{} {}", muted("env:"), report.env_path.display());
    println!("{} {}", muted("binary:"), report.axon_bin.display());
    print_setup_phases(&report.phases);
    Ok(())
}

async fn run_plugin_hook_setup_command(cfg: &Config) -> Result<(), Box<dyn Error>> {
    // The env-var mapping is applied early (before parse_args) by
    // `apply_plugin_options()` in `run()`. Re-applying here is harmless and
    // covers any direct caller that bypasses the early path.
    apply_plugin_options();

    // The SessionStart hook NEVER deploys — provisioning is the `/axon-deploy`
    // slash command. The hook only probes whether the stack is already serving:
    //   - /readyz up   → already deployed; exit silently (success)
    //   - /readyz down → advise running /axon-deploy; exit success (non-blocking)
    // It never runs preflight or `docker compose`.
    if setup::stack_already_healthy().await {
        if cfg.json_output {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "exit_policy": "success",
                    "stack": "already_healthy",
                }))?
            );
        }
        return Ok(());
    }

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "exit_policy": "success",
                "stack": "down",
                "action": "run /axon-deploy",
            }))?
        );
    } else {
        eprintln!("axon stack not reachable on /readyz — run /axon-deploy to start it");
    }
    Ok(())
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
            "token: AXON_HTTP_TOKEN presence is reported in setup phases; values are never printed"
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
    print_setup_phases(&report.phases);
    println!();
    println!("{}", muted("next diagnostic: axon doctor"));
    Ok(())
}

fn print_setup_phases(phases: &[setup::LocalSetupPhase]) {
    for phase in phases {
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
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
