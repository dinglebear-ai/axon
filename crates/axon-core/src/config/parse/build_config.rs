//! Orchestration shim for `into_config()`. The heavy lifting is split across
//! sibling modules (bead axon_rust-2j9.6, no behavior change):
//!
//!   * `command_dispatch`  — translates `CliCommand` → `(CommandKind, positional, accumulators)`.
//!   * `config_literal`    — builds the populated `Config` literal.
//!   * `post_init`         — applies output-path validation and profile defaults.
//!   * `tests`             — split into `env_url_required` + `priority_chain/{ask,tei,workers_search}`.

mod command_dispatch;
mod config_literal;
mod post_init;

#[cfg(test)]
#[path = "build_config_tests.rs"]
pub(crate) mod tests;

use super::super::cli::{Cli, DEFAULT_OUTPUT_DIR};
use super::super::types::{CommandKind, Config};
use super::excludes;
use super::helpers::{default_sqlite_path, parse_viewport, read_env, validate_collection_name};
// AXON_MCP_TRANSPORT is documented as a known knob in docs/guides/configuration.md and is referenced
// inside `config_literal::build` (via `resolve_mcp_transport`) so the
// `cargo xtask check-mcp-http` grep keeps finding the canonical knob name.
use super::toml_config::load_toml_config;

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn into_config(cli: Cli) -> Result<Config, String> {
    into_config_with_sources(cli, false, false)
}

pub(super) fn into_config_with_sources(
    cli: Cli,
    output_dir_was_explicit: bool,
    collection_was_explicit: bool,
) -> Result<Config, String> {
    let mut global = cli.global;

    let dispatched = command_dispatch::dispatch(cli.command);
    // `--watch` is a `global = true` flag, so it lands in `global.watch` for any
    // subcommand. Only `status` and `monitor jobs` implement a live-poll loop;
    // `monitor` reads the global flag via `cfg.watch_mode` (its local `--watch`
    // is shadowed by the global one), so it must be exempted here too.
    if global.watch
        && dispatched.command != CommandKind::Status
        && dispatched.command != CommandKind::Monitor
    {
        return Err(
            "--watch is only supported with `axon status` and `axon monitor jobs`".to_string(),
        );
    }

    if dispatched.command == CommandKind::Reset {
        let toml = load_toml_config()?;
        let collection = if collection_was_explicit {
            global.collection.clone()
        } else {
            read_env("AXON_COLLECTION")
                .or_else(|| toml.search.collection.clone())
                .unwrap_or_else(|| global.collection.clone())
        };
        validate_collection_name(&collection)?;

        let sqlite_path = read_env("AXON_SQLITE_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(default_sqlite_path);

        if !output_dir_was_explicit
            && global.output_dir == std::path::Path::new(DEFAULT_OUTPUT_DIR)
            && let Some(output_dir) = read_env("AXON_OUTPUT_DIR")
        {
            global.output_dir = std::path::PathBuf::from(output_dir);
        }

        let targets_vectors = dispatched.reset_stores.is_empty()
            || dispatched
                .reset_stores
                .iter()
                .any(|store| store == "vectors");
        let qdrant_url = if targets_vectors {
            config_literal::resolve_qdrant_url(&global, &toml)?
        } else {
            config_literal::resolve_qdrant_url(&global, &toml)
                .unwrap_or_else(|_| Config::default().qdrant_url)
        };

        return Ok(Config {
            command: dispatched.command,
            positional: dispatched.positional,
            fresh_action: dispatched.fresh_action,
            json_output: global.json,
            color_choice: global.color,
            watch_mode: global.watch,
            yes: global.yes,
            collection,
            sqlite_path,
            output_dir: global.output_dir,
            qdrant_url,
            reset_stores: dispatched.reset_stores,
            reset_dry_run: dispatched.reset_dry_run,
            reset_plan_id: dispatched.reset_plan_id,
            ..Config::default()
        });
    }

    // Completions and setup metadata/deploy commands do not need service URLs at
    // parse time. Return early so first-run setup works before Qdrant/TEI exist.
    // This means AXON_CONFIG_PATH parse errors and invalid collections are
    // intentionally not checked for these subcommands.
    if matches!(
        dispatched.command,
        CommandKind::Completions
            | CommandKind::Preflight
            | CommandKind::Smoke
            | CommandKind::Compose
            | CommandKind::Setup
            | CommandKind::Config
            | CommandKind::Update
            | CommandKind::Palette
    ) {
        return Ok(Config {
            command: dispatched.command,
            positional: dispatched.positional,
            fresh_action: dispatched.fresh_action,
            json_output: global.json,
            color_choice: global.color,
            watch_mode: global.watch,
            yes: global.yes,
            setup_method: dispatched.setup_method,
            sessions_watch: dispatched.sessions_watch,
            sessions_action: dispatched.sessions_action,
            setup_session_watch_action: dispatched.setup_session_watch_action,
            reset_stores: dispatched.reset_stores,
            reset_dry_run: dispatched.reset_dry_run,
            reset_plan_id: dispatched.reset_plan_id,
            ..Config::default()
        });
    }

    // Load TOML config as the base layer (lowest priority file source).
    // Layer order: CLI flags > env vars > TOML file > hardcoded defaults.
    // Missing file = silent. Malformed file = hard fail with line number.
    let toml = load_toml_config()?;

    // Resolve --collection with priority CLI > env > TOML > "axon".
    // `collection_was_explicit` (from clap's value_source) is the single
    // source of truth for "the user passed --collection on the CLI". The old
    // sentinel `global.collection != "axon"` also caught env-sourced non-default
    // values, causing them to bypass read_env trimming/filtering — that
    // could surface avoidable collection-name validation failures for values
    // like `AXON_COLLECTION=" axon "` (trailing whitespace).
    // Validate the final resolved name regardless of source: it gets
    // interpolated into Qdrant URL paths via format!() with no
    // percent-encoding (CWE-22 — bd axon_rust-d71.6 / H2).
    // Use read_env (trims + filters empty) so a stray `AXON_COLLECTION=""`
    // or `AXON_COLLECTION="   "` falls through to TOML / default rather
    // than failing collection-name validation with an empty name.
    let cli_explicit = collection_was_explicit;
    let collection = if cli_explicit {
        global.collection.clone()
    } else {
        read_env("AXON_COLLECTION")
            .or_else(|| toml.search.collection.clone())
            .unwrap_or_else(|| global.collection.clone())
    };
    validate_collection_name(&collection)?;

    let sqlite_path = read_env("AXON_SQLITE_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(default_sqlite_path);

    if !output_dir_was_explicit
        && global.output_dir == std::path::Path::new(DEFAULT_OUTPUT_DIR)
        && let Some(output_dir) = read_env("AXON_OUTPUT_DIR")
    {
        global.output_dir = std::path::PathBuf::from(output_dir);
    }

    let mut crawl_concurrency_limit = toml.workers.crawl_concurrency_limit;
    let mut backfill_concurrency_limit = toml.workers.backfill_concurrency_limit;
    if let Some(limit) = toml.workers.concurrency_limit {
        crawl_concurrency_limit = Some(limit);
        backfill_concurrency_limit = Some(limit);
    }
    let fetch_retries_was_set = toml.scrape.fetch_retries.is_some();
    let retry_backoff_was_set = toml.scrape.retry_backoff_ms.is_some();

    let exclude_input = std::mem::take(&mut global.exclude_path_prefix);
    let normalized_excludes = excludes::normalize_exclude_prefixes(exclude_input);
    let (viewport_width, viewport_height) = parse_viewport(&global.viewport);

    let inputs = config_literal::LiteralInputs {
        global: &global,
        toml: &toml,
        dispatched: &dispatched,
        collection,
        sqlite_path,
        crawl_concurrency_limit,
        backfill_concurrency_limit,
        exclude_path_prefix: normalized_excludes.prefixes,
        viewport_width,
        viewport_height,
    };
    let mut cfg = config_literal::build(inputs)?;

    post_init::apply(
        &mut cfg,
        post_init::PostInit {
            disable_default_excludes: normalized_excludes.disable_defaults,
            fetch_retries_was_set,
            retry_backoff_was_set,
            output_dir_was_explicit,
        },
    )?;

    Ok(cfg)
}
