//! Bare-`axon <source>` argument routing.
//!
//! Per `docs/pipeline-unification/surfaces/command-contract.md`: *if the first
//! positional token is not a canonical command, a removed command, or a global
//! flag, treat it as `<source>` and route to `SourceRequest`.* So
//! `axon https://x`, `axon ./dir`, `axon r/rust`, and `axon pkg:npm/foo` all
//! index a source with no explicit `source` subcommand, while `axon source <x>`
//! keeps working as an explicit alias.
//!
//! This module rewrites the raw argv **before** clap parses it: it locates the
//! subcommand-position token (skipping leading global flags and any values they
//! consume) and, when that token is not a known subcommand, inserts `source`
//! in front of it. Value-flag awareness is derived from the built clap
//! [`clap::Command`] so it stays in sync with the flag definitions.

use clap::Command;
use std::collections::HashSet;

/// Rewrite `args` (a full argv, `args[0]` = program name) so a bare leading
/// source token is routed through the `source` subcommand.
///
/// Returns the (possibly unchanged) argv. Leaves help/version/empty invocations
/// and explicit subcommands untouched.
pub fn route_bare_source(args: Vec<String>, command: &Command) -> Vec<String> {
    if args.len() < 2 {
        return args;
    }

    let subcommands: HashSet<String> = collect_subcommand_names(command);
    let value_flags: HashSet<String> = collect_value_taking_long_flags(command);

    // Walk the tokens after argv[0], skipping global flags (and the values they
    // consume) until the first positional token — the subcommand position.
    let mut i = 1;
    while i < args.len() {
        let token = &args[i];
        if token == "--" {
            // Everything after `--` is positional; the next token is the
            // subcommand position.
            i += 1;
            break;
        }
        if is_long_flag(token) {
            // `--flag=value` carries its own value; a bare `--flag` that takes a
            // value consumes the next token.
            if !token.contains('=') && flag_takes_value(token, &value_flags) {
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if is_short_flag(token) {
            // Short flags here are the global help (`-h`); treat as valueless.
            i += 1;
            continue;
        }
        // First positional token.
        break;
    }

    if i >= args.len() {
        // Only flags — nothing to route (help/version/global-flag-only).
        return args;
    }

    let candidate = &args[i];
    if subcommands.contains(candidate) || is_help_or_version(candidate) {
        return args;
    }

    // Bare source token — inject `source` before it.
    let mut rewritten = args;
    rewritten.insert(i, "source".to_string());
    rewritten
}

fn collect_subcommand_names(command: &Command) -> HashSet<String> {
    let mut names = HashSet::new();
    for sub in command.get_subcommands() {
        names.insert(sub.get_name().to_string());
        for alias in sub.get_all_aliases() {
            names.insert(alias.to_string());
        }
    }
    names
}

/// The set of `--long` global flags (and their aliases) that consume a value.
///
/// A flag "takes a value" per its [`clap::ArgAction`] (`Set`/`Append`). Boolean
/// flags (`SetTrue`/`SetFalse`/`Count`/help/version) do not consume the next
/// token. `get_action()` is reliable on the pre-`build` command that
/// `Cli::command()` returns.
fn collect_value_taking_long_flags(command: &Command) -> HashSet<String> {
    let mut flags = HashSet::new();
    for arg in command.get_arguments() {
        if !arg.get_action().takes_values() {
            continue;
        }
        if let Some(long) = arg.get_long() {
            flags.insert(format!("--{long}"));
        }
        for alias in arg.get_all_aliases().into_iter().flatten() {
            flags.insert(format!("--{alias}"));
        }
    }
    flags
}

fn is_long_flag(token: &str) -> bool {
    token.starts_with("--") && token.len() > 2
}

fn is_short_flag(token: &str) -> bool {
    token.starts_with('-') && token.len() > 1 && !token.starts_with("--")
}

fn flag_takes_value(token: &str, value_flags: &HashSet<String>) -> bool {
    value_flags.contains(token)
}

fn is_help_or_version(token: &str) -> bool {
    matches!(token, "-h" | "--help" | "help" | "-V" | "--version")
}

#[cfg(test)]
#[path = "source_routing_tests.rs"]
mod tests;
