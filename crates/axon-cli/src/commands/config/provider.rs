//! `axon config provider …` — manage saved LLM provider/model profiles.
//!
//! Profiles live under `[providers.<name>]` in `~/.axon/config.toml`; the active
//! one (`[llm] active-provider`) overrides the per-backend `AXON_*` env vars at
//! resolution time (see `core/config/parse/build_config/provider_overlay.rs`).

use axon_core::config::Config;
use axon_core::ui::{accent, muted, primary};
use axon_llm::LlmBackendKind;
use axon_services::config as svc;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::PathBuf;

/// Settable profile fields. `model`/`cmd`/`home` route to the backend's slot;
/// `base-url`/`api-key` apply to the openai-compat backend.
const FIELDS: &[&str] = &["backend", "model", "base-url", "api-key", "cmd", "home"];

pub(super) fn run_provider(cfg: &Config) -> Result<(), Box<dyn Error>> {
    match cfg.positional.get(1).map(String::as_str) {
        None | Some("list") => provider_list(cfg),
        Some("show") => provider_show(cfg),
        Some("use") => provider_use(cfg),
        Some("add") => provider_add(cfg),
        Some("set") => provider_set(cfg),
        Some("remove") | Some("rm") => provider_remove(cfg),
        Some(other) => Err(format!(
            "unknown provider subcommand {other:?}; expected list|show|use|add|set|remove"
        )
        .into()),
    }
}

fn toml_path() -> Result<PathBuf, Box<dyn Error>> {
    svc::resolve_toml_path().ok_or_else(|| "HOME unset; cannot resolve ~/.axon/config.toml".into())
}

fn arg(cfg: &Config, idx: usize, usage: &str) -> Result<String, Box<dyn Error>> {
    cfg.positional
        .get(idx)
        .cloned()
        .ok_or_else(|| usage.to_string().into())
}

fn validate_field(field: &str) -> Result<(), Box<dyn Error>> {
    if FIELDS.contains(&field) {
        Ok(())
    } else {
        Err(format!(
            "unknown provider field {field:?}; valid: {}",
            FIELDS.join(", ")
        )
        .into())
    }
}

fn provider_names(flat: &BTreeMap<String, String>) -> Vec<String> {
    let mut set = BTreeSet::new();
    for key in flat.keys() {
        if let Some(rest) = key.strip_prefix("providers.")
            && let Some(name) = rest.split('.').next()
        {
            set.insert(name.to_string());
        }
    }
    set.into_iter().collect()
}

fn provider_list(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let doc = svc::read_toml_document(&toml_path()?)?;
    let flat = svc::flatten_toml(&doc);
    let names = provider_names(&flat);
    // Resolve the effective selection through the SAME provider-overlay path the
    // real config uses (single source of truth — see core::config::parse), so
    // this listing can't drift and a broken active profile surfaces as an error
    // instead of a misleading default. `--provider` is a per-run override the
    // `config` command does not carry, so the listing reflects the persisted
    // selection (`AXON_PROVIDER` env > `[llm] active-provider`).
    let eff = axon_core::config::parse::effective_llm(None)?;
    let active = eff.active_provider;
    let effective_backend = match &eff.backend {
        Ok(kind) => kind.as_str().to_string(),
        Err(err) => format!("<unresolved: {err}>"),
    };
    let field = |name: &str, f: &str| flat.get(&format!("providers.{name}.{f}")).cloned();

    if cfg.json_output {
        let list: Vec<_> = names
            .iter()
            .map(|n| {
                json!({
                    "name": n,
                    "backend": field(n, "backend"),
                    "model": field(n, "model"),
                    "active": active.as_deref() == Some(n),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "providers": list,
                "active_provider": active,
                "effective_backend": effective_backend,
            }))?
        );
        return Ok(());
    }

    println!("{}", primary("Providers"));
    if names.is_empty() {
        println!(
            "  {}",
            muted("(none — add one with `axon config provider add <name> <backend>`)")
        );
    }
    for name in &names {
        let marker = if active.as_deref() == Some(name) {
            accent("● ")
        } else {
            "  ".to_string()
        };
        let backend = field(name, "backend").unwrap_or_else(|| "?".to_string());
        let model = field(name, "model")
            .map(|m| format!(" ({m})"))
            .unwrap_or_default();
        println!(
            "  {marker}{} {}{}",
            accent(name),
            muted(&backend),
            muted(&model)
        );
    }
    println!();
    println!(
        "{} {}",
        primary("active:"),
        active.as_deref().map_or_else(|| muted("(none)"), accent)
    );
    println!(
        "{} {}",
        primary("effective backend:"),
        accent(&effective_backend)
    );
    Ok(())
}

fn provider_show(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let name = arg(cfg, 2, "axon config provider show <name>: missing name")?;
    let reveal = cfg.positional.iter().any(|a| a == "--reveal");
    let doc = svc::read_toml_document(&toml_path()?)?;
    let prefix = format!("providers.{name}.");
    let fields: Vec<(String, String)> = svc::flatten_toml(&doc)
        .into_iter()
        .filter_map(|(k, v)| k.strip_prefix(&prefix).map(|f| (f.to_string(), v)))
        .collect();
    if fields.is_empty() {
        return Err(format!("provider '{name}' not found").into());
    }
    let shown = |field: &str, value: &str| {
        let key = format!("{prefix}{field}");
        if !reveal && svc::is_secret_toml_key(&key) {
            svc::redact(value)
        } else {
            value.to_string()
        }
    };

    if cfg.json_output {
        let map: serde_json::Map<_, _> = fields
            .iter()
            .map(|(f, v)| (f.clone(), json!(shown(f, v))))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "name": name, "fields": map }))?
        );
        return Ok(());
    }
    println!("{} {}", primary("provider"), accent(&name));
    for (f, v) in &fields {
        println!("  {} = {}", accent(f), shown(f, v));
    }
    Ok(())
}

fn provider_use(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let name = arg(cfg, 2, "axon config provider use <name>: missing name")?;
    let path = toml_path()?;
    let mut doc = svc::read_toml_document(&path)?;
    let backend = svc::get_toml_entry(&doc, &format!("providers.{name}.backend")).ok_or_else(
        || {
            format!(
                "provider '{name}' not found (or missing backend); add it with `axon config provider add {name} <backend>`"
            )
        },
    )?;
    LlmBackendKind::parse(backend.trim())
        .map_err(|err| format!("provider '{name}' has an invalid backend: {err}"))?;
    svc::set_toml_entry(&mut doc, "llm.active-provider", &name)?;
    svc::write_toml_document(&path, &doc)?;
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "active_provider": name,
                "backend": backend.trim(),
                "status": "set",
            }))?
        );
    } else {
        println!(
            "{} {} {}",
            primary("active provider →"),
            accent(&name),
            muted(&format!("({})", backend.trim()))
        );
    }
    Ok(())
}

fn provider_add(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let usage = "axon config provider add <name> <backend> [field=value ...]";
    let name = arg(cfg, 2, &format!("{usage}: missing name"))?;
    let backend = arg(cfg, 3, &format!("{usage}: missing backend"))?;
    LlmBackendKind::parse(backend.trim())
        .map_err(|err| format!("invalid backend {backend:?}: {err}"))?;
    let path = toml_path()?;
    let mut doc = svc::read_toml_document(&path)?;
    svc::set_toml_entry(
        &mut doc,
        &format!("providers.{name}.backend"),
        backend.trim(),
    )?;
    for pair in cfg.positional.iter().skip(4) {
        let (field, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("expected field=value, got {pair:?}"))?;
        validate_field(field)?;
        if field == "backend" {
            // A `backend=` override must be a real backend too — otherwise it
            // silently overwrites the validated positional backend with garbage.
            LlmBackendKind::parse(value.trim())
                .map_err(|err| format!("invalid backend {value:?}: {err}"))?;
        }
        svc::set_toml_entry(&mut doc, &format!("providers.{name}.{field}"), value)?;
    }
    svc::write_toml_document(&path, &doc)?;
    report_set(cfg, &name, "saved", &path)
}

fn provider_set(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let usage = "axon config provider set <name> <field> <value>";
    let name = arg(cfg, 2, &format!("{usage}: missing name"))?;
    let field = arg(cfg, 3, &format!("{usage}: missing field"))?;
    let value = arg(cfg, 4, &format!("{usage}: missing value"))?;
    validate_field(&field)?;
    if field == "backend" {
        LlmBackendKind::parse(value.trim())
            .map_err(|err| format!("invalid backend {value:?}: {err}"))?;
    }
    let path = toml_path()?;
    let mut doc = svc::read_toml_document(&path)?;
    // `set` edits an existing profile; refuse to create an orphan section. A
    // profile with no backend would fail every later `use`/`ask`, so a typo'd
    // name should error here, not silently report success.
    if svc::get_toml_entry(&doc, &format!("providers.{name}.backend")).is_none() {
        return Err(format!(
            "provider '{name}' not found; create it first with `axon config provider add {name} <backend>`"
        )
        .into());
    }
    svc::set_toml_entry(&mut doc, &format!("providers.{name}.{field}"), &value)?;
    svc::write_toml_document(&path, &doc)?;
    report_set(cfg, &name, "updated", &path)
}

fn provider_remove(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let name = arg(cfg, 2, "axon config provider remove <name>: missing name")?;
    let path = toml_path()?;
    let mut doc = svc::read_toml_document(&path)?;
    let removed = svc::unset_toml_entry(&mut doc, &format!("providers.{name}"))?;
    let was_active = svc::get_toml_entry(&doc, "llm.active-provider")
        .as_deref()
        .map(str::trim)
        == Some(&name);
    if was_active {
        svc::unset_toml_entry(&mut doc, "llm.active-provider")?;
    }
    if removed || was_active {
        svc::write_toml_document(&path, &doc)?;
    }
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "name": name,
                "status": if removed { "removed" } else { "not_found" },
                "cleared_active": was_active,
            }))?
        );
    } else if removed {
        let note = if was_active { " (was active)" } else { "" };
        println!(
            "{} {}{}",
            primary("removed provider"),
            accent(&name),
            muted(note)
        );
    } else {
        println!("{} {}", muted("not present:"), accent(&name));
    }
    Ok(())
}

fn report_set(
    cfg: &Config,
    name: &str,
    status: &str,
    path: &std::path::Path,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "name": name,
                "status": status,
                "path": path.display().to_string(),
            }))?
        );
    } else {
        println!(
            "{} {} {}",
            primary(&format!("provider {status}:")),
            accent(name),
            muted(&format!("→ {}", path.display()))
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
