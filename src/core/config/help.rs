use super::cli::Cli;
use crate::core::ui::{ACCENT_ANSI, PRIMARY_ANSI, ansi_bold, ansi_colorize, ansi_dim};
use clap::{Arg, ArgAction, Command, CommandFactory};
use std::collections::HashSet;
use std::env;
use std::path::Path;
use std::process;

const COMMAND_SECTIONS: &[(&str, &[&str])] = &[
    (
        "Web And Extraction",
        &[
            "scrape",
            "crawl",
            "map",
            "endpoints",
            "search",
            "research",
            "extract",
            "screenshot",
            "diff",
            "brand",
        ],
    ),
    (
        "Vector And RAG",
        &[
            "embed",
            "query",
            "code-search",
            "retrieve",
            "ask",
            "evaluate",
            "train",
            "summarize",
            "suggest",
            "memory",
            "sources",
            "domains",
            "stats",
            "dedupe",
            "purge",
            "migrate",
        ],
    ),
    (
        "Jobs And Imports",
        &[
            "status", "ingest", "sessions", "watch", "monitor", "sync", "refresh",
        ],
    ),
    (
        "Runtime And Setup",
        &[
            "debug",
            "doctor",
            "mcp",
            "serve",
            "setup",
            "preflight",
            "smoke",
            "compose",
            "completions",
            "config",
            "update",
            "palette",
        ],
    ),
];

const VECTOR_OPTIONS: &[(&str, &str)] = &[
    ("--collection <name>", "Qdrant collection name"),
    ("--limit <n>", "Maximum number of results"),
    (
        "--since <date>",
        "Filter to content indexed on or after this date",
    ),
    (
        "--before <date>",
        "Filter to content indexed on or before this date",
    ),
    ("--no-hybrid-search", "Force dense-only retrieval"),
    ("--json", "Output machine-readable JSON"),
];

const EMBED_OPTIONS: &[(&str, &str)] = &[
    ("--collection <name>", "Qdrant collection name"),
    ("--wait <bool>", "Block until the embed job completes"),
    (
        "--batch-concurrency <n>",
        "Concurrent embed batch operations",
    ),
    ("--tei-url <url>", "Text Embeddings Inference endpoint"),
    ("--qdrant-url <url>", "Qdrant endpoint"),
    ("--json", "Output machine-readable JSON"),
];

const WEB_OPTIONS: &[(&str, &str)] = &[
    (
        "--max-pages <n>",
        "Maximum pages to crawl (crawl default 2000; 0 = uncapped)",
    ),
    ("--max-depth <n>", "Maximum crawl depth"),
    (
        "--render-mode <mode>",
        "Page fetch mode: http, chrome, or auto-switch",
    ),
    (
        "--include-subdomains <bool>",
        "Include subdomains in crawl scope",
    ),
    ("--header <HEADER>", "Custom HTTP request header"),
    ("--skip-embed", "Fetch/save without indexing into Qdrant"),
    ("--collection <name>", "Qdrant collection name"),
    ("--wait <bool>", "Block until async jobs complete"),
    ("--json", "Output machine-readable JSON"),
];

const SEARCH_OPTIONS: &[(&str, &str)] = &[
    ("--limit <n>", "Maximum number of search results"),
    (
        "--search-time-range <range>",
        "Restrict search to day, week, month, or year",
    ),
    ("--json", "Output machine-readable JSON"),
];

const JOB_VIEW_OPTIONS: &[(&str, &str)] = &[
    ("--active", "Show only active jobs"),
    ("--recent", "Show active and completed jobs"),
    ("--reclaimed", "Show only watchdog-reclaimed jobs"),
    ("--json", "Output machine-readable JSON"),
];

const SERVICE_OPTIONS: &[(&str, &str)] = &[
    ("--tei-url <url>", "Text Embeddings Inference endpoint"),
    ("--qdrant-url <url>", "Qdrant endpoint"),
    ("--json", "Output machine-readable JSON"),
];

pub(super) fn maybe_print_top_level_help_and_exit() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 2 && matches!(args[1].as_str(), "-h" | "--help" | "help") {
        print_top_level_help();
        process::exit(0);
    }
    if args.len() == 3
        && matches!(args[2].as_str(), "-h" | "--help" | "help")
        && print_command_help(&args[1])
    {
        process::exit(0);
    }
    if args.len() > 3
        && matches!(
            args.last().map(String::as_str),
            Some("-h" | "--help" | "help")
        )
    {
        let path: Vec<&str> = args[1..args.len() - 1].iter().map(String::as_str).collect();
        if print_command_path_help(&path) {
            process::exit(0);
        }
    }
}

struct Palette {}

impl Palette {
    fn new() -> Self {
        Self {}
    }

    fn colorize(&self, code: &str, text: &str) -> String {
        ansi_colorize(code, text)
    }

    fn bold(&self, text: &str) -> String {
        ansi_bold(text)
    }

    fn dim(&self, text: &str) -> String {
        ansi_dim(text)
    }

    fn primary(&self, text: &str) -> String {
        self.colorize(PRIMARY_ANSI, text)
    }

    fn accent(&self, text: &str) -> String {
        self.colorize(ACCENT_ANSI, text)
    }

    fn section(&self, name: &str) -> String {
        self.bold(&self.primary(name))
    }
}

fn binary_name() -> String {
    env::args()
        .next()
        .as_deref()
        .and_then(|p| Path::new(p).file_name().and_then(|s| s.to_str()))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "axon".to_string())
}

fn print_top_level_help() {
    let p = Palette::new();
    let bin = binary_name();

    let row = |flag: &str, desc: &str| print_row(&p, 2, 28, flag, desc);
    let command_row = |name: &str, desc: &str| print_row(&p, 4, 20, name, desc);

    println!("  {}", p.bold(&p.primary("AXON CLI")));
    println!("  {}", p.primary("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"));
    println!(
        "  Version {}  |  {}",
        env!("CARGO_PKG_VERSION"),
        p.dim("Spider-powered web and local RAG CLI")
    );
    println!();

    println!("  {}", p.section("Usage"));
    println!("  {}", p.accent(&format!("[{bin} [options] [command]]")));
    println!();

    println!("  {}", p.section("Quick Start"));
    for example in [
        format!("{bin} scrape https://example.com --wait true --skip-embed"),
        format!("{bin} crawl https://docs.rs/spider --wait false"),
        format!("{bin} query \"embedding pipeline\" --collection axon"),
    ] {
        println!("  {}", p.dim(&example));
    }
    println!();

    println!("  {}", p.section("Global Options"));
    row("-h, --help", "display help");
    row("--wait <bool>", "true waits; false enqueues and returns");
    row("--collection <name>", "vector collection (default axon)");
    row("--skip-embed", "fetch/save without indexing into Qdrant");
    row(
        "--cache <bool>",
        "reuse prior crawl artifacts when possible",
    );
    row(
        "--cache-http-only",
        "keep cached crawl flow on the HTTP path",
    );
    row(
        "--max-pages <n>",
        "crawl page limit (default 2000; 0 = uncapped)",
    );
    row(
        "--url-glob <pattern[,..]>",
        "expand URL seeds via brace globs (e.g. {1..10}, {a,b})",
    );
    row("--cron-every-seconds <n>", "repeat command every n seconds");
    row("--cron-max-runs <n>", "stop cron loop after n runs");
    row("--max-depth <n>", "crawl depth");
    row("--output-dir <dir>", "output directory");
    println!();

    println!("  {}", p.section("Commands"));
    let commands = command_rows();
    let mut printed = HashSet::new();
    for (section, names) in COMMAND_SECTIONS {
        println!("  {}", p.accent(section));
        for name in *names {
            if let Some((_, about)) = commands.iter().find(|(command, _)| command == name) {
                command_row(name, about);
                printed.insert(*name);
            }
        }
        println!();
    }

    let uncategorized: Vec<_> = commands
        .iter()
        .filter(|(name, _)| !printed.contains(name.as_str()))
        .collect();
    if !uncategorized.is_empty() {
        println!("  {}", p.accent("Other"));
        for (name, about) in uncategorized {
            command_row(name, about);
        }
        println!();
    }

    println!(
        "  {}",
        p.dim(&format!(
            "→ Run {bin} <command> --help for command-specific flags"
        ))
    );
}

fn print_command_help(command_name: &str) -> bool {
    print_command_path_help(&[command_name])
}

fn print_command_path_help(path: &[&str]) -> bool {
    let command = Cli::command();
    let mut current = &command;
    for name in path {
        let Some(subcommand) = current
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == *name)
        else {
            return false;
        };
        current = subcommand;
    }

    render_command_help(current, path);
    true
}

fn render_command_help(command: &Command, path: &[&str]) {
    let p = Palette::new();
    let about = command_about(command);
    let arguments = local_arguments(command);
    let options = command_options(command, path);
    let subcommands = local_subcommands(command);

    println!("{}", p.bold(&p.primary(&about)));
    println!();
    println!("{}", p.section("Usage"));
    for usage in command_usage(path, &arguments, &options, &subcommands) {
        println!("  {}", p.accent(&usage));
    }

    if !subcommands.is_empty() {
        println!();
        println!("{}", p.section("Commands"));
        for (name, desc) in subcommands {
            print_row(&p, 2, 20, &name, &desc);
        }
    }

    if !arguments.is_empty() {
        println!();
        println!("{}", p.section("Arguments"));
        for (name, desc) in arguments {
            print_row(&p, 2, 18, &name, &desc);
        }
    }

    if !options.is_empty() {
        println!();
        println!("{}", p.section("Options"));
        for (name, desc) in options {
            print_row(&p, 2, 32, &name, &desc);
        }
    }
}

fn command_about(command: &Command) -> String {
    command
        .get_about()
        .or_else(|| command.get_long_about())
        .map(ToString::to_string)
        .unwrap_or_else(|| command.get_name().to_string())
}

fn command_usage(
    path: &[&str],
    arguments: &[(String, String)],
    options: &[(String, String)],
    subcommands: &[(String, String)],
) -> Vec<String> {
    let command_path = path.join(" ");
    let mut base = format!("axon {command_path}");
    if !options.is_empty() {
        base.push_str(" [OPTIONS]");
    }
    for (arg, _) in arguments {
        base.push(' ');
        base.push_str(arg);
    }
    let mut usage = vec![base];

    if !subcommands.is_empty() {
        usage.push(format!("axon {command_path} <COMMAND>"));
    }
    usage
}

fn local_subcommands(command: &Command) -> Vec<(String, String)> {
    command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set() && subcommand.get_name() != "help")
        .map(|subcommand| {
            let name = subcommand_name(command.get_name(), subcommand);
            let desc = subcommand_description(command.get_name(), subcommand);
            (name, desc)
        })
        .collect()
}

fn subcommand_description(parent_name: &str, command: &Command) -> String {
    if matches!(
        parent_name,
        "crawl" | "extract" | "embed" | "ingest" | "sessions"
    ) {
        return match command.get_name() {
            "status" => "Show a queued job".to_string(),
            "cancel" => "Cancel a queued or running job".to_string(),
            "errors" => "Show job error details".to_string(),
            "list" => "List recent jobs".to_string(),
            "cleanup" => "Remove old terminal jobs".to_string(),
            "clear" => "Clear job history".to_string(),
            "worker" => "Run an inline worker".to_string(),
            "recover" => "Reclaim stale or interrupted jobs".to_string(),
            _ => command_about(command),
        };
    }

    match (parent_name, command.get_name()) {
        ("watch", "create") => "Create a recurring watch definition".to_string(),
        ("watch", "list") => "List watch definitions".to_string(),
        ("watch", "get") => "Show a watch definition".to_string(),
        ("watch", "update") => "Update a watch definition".to_string(),
        ("watch", "run-now") => "Run a watch immediately".to_string(),
        ("watch", "pause") => "Pause a watch".to_string(),
        ("watch", "resume") => "Resume a paused watch".to_string(),
        ("watch", "delete") => "Delete a watch definition".to_string(),
        ("watch", "history") => "Show watch run history".to_string(),
        ("watch", "artifacts") => "List artifacts for a watch run".to_string(),
        ("monitor", "jobs") => "Emit job lifecycle events".to_string(),
        _ => command_about(command),
    }
}

fn subcommand_name(parent_name: &str, command: &Command) -> String {
    match (parent_name, command.get_name()) {
        ("crawl" | "extract" | "embed" | "ingest" | "sessions", "status" | "cancel" | "errors") => {
            format!("{} <job_id>", command.get_name())
        }
        _ => command.get_name().to_string(),
    }
}

fn local_arguments(command: &Command) -> Vec<(String, String)> {
    command
        .get_arguments()
        .filter(|arg| is_local_visible_arg(arg) && is_positional_arg(arg))
        .map(|arg| (argument_label(arg), arg_help(arg)))
        .collect()
}

fn local_options(command: &Command) -> Vec<(String, String)> {
    command
        .get_arguments()
        .filter(|arg| is_local_visible_arg(arg) && !is_positional_arg(arg))
        .map(|arg| (option_label(arg), arg_help(arg)))
        .collect()
}

fn command_options(command: &Command, path: &[&str]) -> Vec<(String, String)> {
    let mut options = local_options(command);
    options.extend(relevant_global_options(command.get_name(), path));

    options.push(("-h, --help".to_string(), "Print help".to_string()));
    options
}

fn relevant_global_options(command_name: &str, path: &[&str]) -> Vec<(String, String)> {
    if matches!(path, ["setup", "session-watch-service", "status"]) {
        return Vec::new();
    }
    let specs: &[(&str, &str)] = match command_name {
        "scrape" | "crawl" | "extract" | "map" | "screenshot" | "diff" | "brand" => WEB_OPTIONS,
        "search" => SEARCH_OPTIONS,
        "research" => &[
            ("--limit <n>", "Maximum number of search results"),
            (
                "--research-depth <n>",
                "Number of sources to synthesize over",
            ),
            (
                "--search-time-range <range>",
                "Restrict search to day, week, month, or year",
            ),
            ("--skip-embed", "Queue crawls without indexing into Qdrant"),
            ("--json", "Output machine-readable JSON"),
        ],
        "embed" => EMBED_OPTIONS,
        "query" | "retrieve" | "ask" | "evaluate" | "train" | "sources" | "domains" | "stats"
        | "dedupe" | "migrate" | "suggest" => VECTOR_OPTIONS,
        "status" => JOB_VIEW_OPTIONS,
        "ingest" | "sessions" | "watch" => &[
            ("--wait <bool>", "Block until async jobs complete"),
            ("--active", "Show only active jobs"),
            ("--recent", "Show active and completed jobs"),
            ("--json", "Output machine-readable JSON"),
        ],
        "refresh" => &[
            ("--yes", "Skip the confirmation prompt before re-enqueuing"),
            ("--json", "Output machine-readable JSON"),
        ],
        "debug" | "doctor" => SERVICE_OPTIONS,
        "mcp" => &[],
        "serve" => &[("--transport <mode>", "MCP transport for `serve mcp`")],
        _ => &[],
    };

    specs
        .iter()
        .map(|(label, desc)| ((*label).to_string(), (*desc).to_string()))
        .collect()
}

fn is_local_visible_arg(arg: &Arg) -> bool {
    !arg.is_hide_set() && !arg.is_global_set()
}

fn is_positional_arg(arg: &Arg) -> bool {
    arg.get_index().is_some() || (arg.get_short().is_none() && arg.get_long().is_none())
}

fn argument_label(arg: &Arg) -> String {
    let label = value_label(arg);
    let mut formatted = if arg.is_required_set() {
        format!("<{label}>")
    } else {
        format!("[{label}]")
    };
    if matches!(arg.get_action(), ArgAction::Append) {
        formatted.push_str("...");
    }
    formatted
}

fn option_label(arg: &Arg) -> String {
    let mut label = match (arg.get_short(), arg.get_long()) {
        (Some(short), Some(long)) => format!("-{short}, --{long}"),
        (Some(short), None) => format!("-{short}"),
        (None, Some(long)) => format!("--{long}"),
        (None, None) => arg.get_id().as_str().to_string(),
    };

    if option_takes_value(arg) {
        label.push(' ');
        label.push('<');
        label.push_str(&value_label(arg));
        label.push('>');
    }
    label
}

fn option_takes_value(arg: &Arg) -> bool {
    matches!(arg.get_action(), ArgAction::Set | ArgAction::Append)
}

fn value_label(arg: &Arg) -> String {
    arg.get_value_names()
        .and_then(|names| names.first())
        .map(ToString::to_string)
        .unwrap_or_else(|| arg.get_id().as_str().to_uppercase())
}

fn arg_help(arg: &Arg) -> String {
    let help = arg
        .get_help()
        .or_else(|| arg.get_long_help())
        .map(ToString::to_string)
        .unwrap_or_default();
    if !help.is_empty() {
        return help;
    }

    match value_label(arg).as_str() {
        "INPUT" => "File, directory, URL, or raw text to embed".to_string(),
        "URL" => "URL to process".to_string(),
        "TEXT" => "Text query or prompt".to_string(),
        _ => String::new(),
    }
}

fn print_row(p: &Palette, indent: usize, label_width: usize, label: &str, desc: &str) {
    if label.chars().count() > label_width {
        println!("{:indent$}{}", "", p.accent(label), indent = indent);
        println!(
            "{:indent$}{}",
            "",
            p.dim(desc),
            indent = indent + label_width + 1
        );
        return;
    }

    let padded_label = format!("{label:<label_width$}");
    println!(
        "{:indent$}{} {}",
        "",
        p.accent(&padded_label),
        p.dim(desc),
        indent = indent
    );
}

fn command_rows() -> Vec<(String, String)> {
    Cli::command()
        .get_subcommands()
        .filter(|command| !command.is_hide_set())
        .map(|command| {
            let about = command
                .get_about()
                .or_else(|| command.get_long_about())
                .map(ToString::to_string)
                .unwrap_or_default();
            (command.get_name().to_string(), about)
        })
        .collect()
}

#[cfg(test)]
#[path = "help_tests.rs"]
mod tests;
