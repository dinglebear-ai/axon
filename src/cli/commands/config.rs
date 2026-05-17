use crate::core::config::Config;
use crate::core::ui::{accent, muted, primary};
use crate::services::config as svc;
use serde_json::json;
use std::error::Error;
use std::path::PathBuf;

const USAGE_LINES: &[&str] = &[
    "axon config list [--env] [--toml] [--reveal]",
    "axon config get <key> [--env|--toml] [--reveal]",
    "axon config set <key> <value> [--env|--toml]",
    "axon config unset <key> [--env|--toml]",
    "axon config path",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Env,
    Toml,
}

pub async fn run_config(cfg: &Config) -> Result<(), Box<dyn Error>> {
    match cfg.positional.first().map(String::as_str) {
        None | Some("list") => run_list(cfg),
        Some("get") => run_get(cfg),
        Some("set") => run_set(cfg),
        Some("unset") => run_unset(cfg),
        Some("path") => run_path(cfg),
        _ => print_usage(cfg),
    }
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

fn run_path(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let env_path = svc::resolve_env_path();
    let toml_path = svc::resolve_toml_path();
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "env_path": env_path.as_ref().map(|p| p.display().to_string()),
                "toml_path": toml_path.as_ref().map(|p| p.display().to_string()),
            }))?
        );
        return Ok(());
    }
    println!(
        "{} {}",
        primary(".env:"),
        accent(&path_display(env_path.as_ref()))
    );
    println!(
        "{} {}",
        primary("config.toml:"),
        accent(&path_display(toml_path.as_ref()))
    );
    Ok(())
}

fn run_list(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let flags = parse_list_flags(&cfg.positional);
    let show_env = flags.env || !flags.toml;
    let show_toml = flags.toml || !flags.env;

    let env_entries = if show_env {
        match svc::resolve_env_path() {
            Some(path) => svc::read_env_entries(&path)?,
            None => Default::default(),
        }
    } else {
        Default::default()
    };

    let toml_entries = if show_toml {
        match svc::resolve_toml_path() {
            Some(path) => svc::flatten_toml(&svc::read_toml_document(&path)?),
            None => Default::default(),
        }
    } else {
        Default::default()
    };

    if cfg.json_output {
        let env_view: serde_json::Map<_, _> = env_entries
            .iter()
            .map(|(k, v)| (k.clone(), json!(display_env_value(k, v, flags.reveal))))
            .collect();
        let toml_view: serde_json::Map<_, _> = toml_entries
            .iter()
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "env": env_view,
                "toml": toml_view,
            }))?
        );
        return Ok(());
    }

    if show_env {
        println!("{}", primary(".env"));
        if env_entries.is_empty() {
            println!("  {}", muted("(no entries)"));
        } else {
            for (key, value) in &env_entries {
                println!(
                    "  {} = {}",
                    accent(key),
                    display_env_value(key, value, flags.reveal)
                );
            }
        }
    }

    if show_toml {
        if show_env {
            println!();
        }
        println!("{}", primary("config.toml"));
        if toml_entries.is_empty() {
            println!("  {}", muted("(no entries)"));
        } else {
            for (key, value) in &toml_entries {
                println!("  {} = {value}", accent(key));
            }
        }
    }
    Ok(())
}

fn run_get(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let key = cfg
        .positional
        .get(1)
        .ok_or("axon config get <key>: missing key")?;
    let flags = cfg.positional.iter().skip(2);
    let force_env = flags.clone().any(|v| v == "--env");
    let force_toml = flags.clone().any(|v| v == "--toml");
    let reveal = flags.clone().any(|v| v == "--reveal");
    let target = detect_target(key, force_env, force_toml)?;

    let (raw_value, source) = match target {
        Target::Env => {
            let path = svc::resolve_env_path().ok_or("HOME unset; cannot resolve ~/.axon/.env")?;
            let value = svc::read_env_entries(&path)?.remove(key);
            (value, "env")
        }
        Target::Toml => {
            let path =
                svc::resolve_toml_path().ok_or("HOME unset; cannot resolve ~/.axon/config.toml")?;
            let document = svc::read_toml_document(&path)?;
            (svc::get_toml_entry(&document, key), "toml")
        }
    };

    let display = raw_value.as_deref().map(|v| match target {
        Target::Env => display_env_value(key, v, reveal),
        Target::Toml => v.to_string(),
    });

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "key": key,
                "source": source,
                "value": display,
                "present": raw_value.is_some(),
            }))?
        );
    } else if let Some(value) = display {
        println!("{value}");
    } else {
        return Err(format!("{key}: not set in {source}").into());
    }
    Ok(())
}

fn run_set(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let key = cfg
        .positional
        .get(1)
        .ok_or("axon config set <key> <value>: missing key")?;
    let value = cfg
        .positional
        .get(2)
        .ok_or("axon config set <key> <value>: missing value")?;
    let flags = cfg.positional.iter().skip(3);
    let force_env = flags.clone().any(|v| v == "--env");
    let force_toml = flags.clone().any(|v| v == "--toml");
    let target = detect_target(key, force_env, force_toml)?;

    let path = write_target(target, key, value)?;

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "key": key,
                "target": target_label(target),
                "path": path.display().to_string(),
                "status": "set",
            }))?
        );
    } else {
        println!(
            "{} {} = {} {}",
            primary("set"),
            accent(key),
            display_env_value(key, value, false),
            muted(&format!("→ {}", path.display())),
        );
    }
    Ok(())
}

fn run_unset(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let key = cfg
        .positional
        .get(1)
        .ok_or("axon config unset <key>: missing key")?;
    let flags = cfg.positional.iter().skip(2);
    let force_env = flags.clone().any(|v| v == "--env");
    let force_toml = flags.clone().any(|v| v == "--toml");
    let target = detect_target(key, force_env, force_toml)?;

    let (path, removed) = match target {
        Target::Env => {
            let path = svc::resolve_env_path().ok_or("HOME unset; cannot resolve ~/.axon/.env")?;
            let removed = svc::unset_env_entry(&path, key)?;
            (path, removed)
        }
        Target::Toml => {
            let path =
                svc::resolve_toml_path().ok_or("HOME unset; cannot resolve ~/.axon/config.toml")?;
            let mut document = svc::read_toml_document(&path)?;
            let removed = svc::unset_toml_entry(&mut document, key)?;
            if removed {
                svc::write_toml_document(&path, &document)?;
            }
            (path, removed)
        }
    };

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "key": key,
                "target": target_label(target),
                "path": path.display().to_string(),
                "status": if removed { "removed" } else { "not_found" },
            }))?
        );
    } else if removed {
        println!(
            "{} {} {}",
            primary("unset"),
            accent(key),
            muted(&format!("→ {}", path.display())),
        );
    } else {
        println!("{} {}", muted("not present:"), accent(key));
    }
    Ok(())
}

fn write_target(target: Target, key: &str, value: &str) -> Result<PathBuf, Box<dyn Error>> {
    match target {
        Target::Env => {
            let path = svc::resolve_env_path().ok_or("HOME unset; cannot resolve ~/.axon/.env")?;
            svc::set_env_entry(&path, key, value)?;
            Ok(path)
        }
        Target::Toml => {
            let path =
                svc::resolve_toml_path().ok_or("HOME unset; cannot resolve ~/.axon/config.toml")?;
            let mut document = svc::read_toml_document(&path)?;
            svc::set_toml_entry(&mut document, key, value)?;
            svc::write_toml_document(&path, &document)?;
            Ok(path)
        }
    }
}

fn detect_target(key: &str, force_env: bool, force_toml: bool) -> Result<Target, Box<dyn Error>> {
    if force_env && force_toml {
        return Err("--env and --toml are mutually exclusive".into());
    }
    if force_env {
        return Ok(Target::Env);
    }
    if force_toml {
        return Ok(Target::Toml);
    }
    let first = key.chars().next();
    let looks_env = first.is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && key
            .chars()
            .all(|c| c == '_' || c.is_ascii_uppercase() || c.is_ascii_digit())
        && !key.contains('.');
    let looks_toml = key.contains('.')
        && key.chars().all(|c| {
            c == '.' || c == '-' || c == '_' || c.is_ascii_lowercase() || c.is_ascii_digit()
        });
    match (looks_env, looks_toml) {
        (true, false) => Ok(Target::Env),
        (false, true) => Ok(Target::Toml),
        _ => Err(format!(
            "cannot infer target for {key:?}: pass --env (UPPER_SNAKE) or --toml (dotted.lower)"
        )
        .into()),
    }
}

struct ListFlags {
    env: bool,
    toml: bool,
    reveal: bool,
}

fn parse_list_flags(positional: &[String]) -> ListFlags {
    let mut flags = positional.iter().skip(1);
    ListFlags {
        env: flags.clone().any(|v| v == "--env"),
        toml: flags.clone().any(|v| v == "--toml"),
        reveal: flags.any(|v| v == "--reveal"),
    }
}

fn display_env_value(key: &str, value: &str, reveal: bool) -> String {
    if !reveal && svc::is_secret_env_key(key) {
        svc::redact(value)
    } else {
        value.to_string()
    }
}

fn target_label(target: Target) -> &'static str {
    match target {
        Target::Env => "env",
        Target::Toml => "toml",
    }
}

fn path_display(path: Option<&PathBuf>) -> String {
    match path {
        Some(p) => p.display().to_string(),
        None => "<HOME unset>".to_string(),
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
