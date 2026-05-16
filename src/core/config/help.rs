use std::env;
use std::path::Path;
use std::process;

const PRIMARY: &str = "\x1b[38;2;244;143;177m";
const ACCENT: &str = "\x1b[38;2;144;202;249m";

pub(super) fn maybe_print_top_level_help_and_exit() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 2 && matches!(args[1].as_str(), "-h" | "--help" | "help") {
        print_top_level_help();
        process::exit(0);
    }
    if args.len() == 3 && args[1] == "setup" && matches!(args[2].as_str(), "-h" | "--help" | "help")
    {
        print_setup_help();
        process::exit(0);
    }
}

struct Palette {
    enabled: bool,
}

impl Palette {
    fn new() -> Self {
        Self {
            enabled: env::var_os("NO_COLOR").is_none(),
        }
    }

    fn colorize(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("{code}{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn bold(&self, text: &str) -> String {
        self.colorize("\x1b[1m", text)
    }

    fn dim(&self, text: &str) -> String {
        self.colorize("\x1b[2m", text)
    }

    fn primary(&self, text: &str) -> String {
        self.colorize(PRIMARY, text)
    }

    fn accent(&self, text: &str) -> String {
        self.colorize(ACCENT, text)
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

    let row = |flag: &str, desc: &str| {
        println!("  {:<28} {}", p.accent(flag), p.dim(desc));
    };

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
        format!("{bin} scrape https://example.com --wait true --embed false"),
        format!("{bin} crawl https://docs.rs/spider --wait false"),
        format!("{bin} query \"embedding pipeline\" --collection cortex"),
    ] {
        println!("  {}", p.dim(&example));
    }
    println!();

    println!("  {}", p.section("Global Options"));
    row("-h, --help", "display help");
    row("--wait <bool>", "true waits; false enqueues and returns");
    row("--collection <name>", "vector collection (default cortex)");
    row("--embed <bool>", "run embedding where applicable");
    row(
        "--cache <bool>",
        "reuse prior crawl artifacts when possible",
    );
    row(
        "--cache-skip-browser <bool>",
        "force HTTP crawl path when cache flow is enabled",
    );
    row("--max-pages <n>", "crawl page limit (0 = uncapped)");
    row(
        "--url-glob <pattern[,..]>",
        "expand URL seeds via brace globs (e.g. {1..10}, {a,b})",
    );
    row("--cron-every-seconds <n>", "repeat command every n seconds");
    row("--cron-max-runs <n>", "stop cron loop after n runs");
    row("--max-depth <n>", "crawl depth");
    row("--output-dir <dir>", "output directory");
    println!();

    println!("  {}", p.section("Core Web Operations"));
    row("scrape [url]", "Scrape a URL");
    row("crawl [url]", "Crawl a website");
    row("map [url]", "Map URLs on a website");
    row("search <query>", "Search web results");
    row("extract [urls...]", "Extract structured data");
    println!();

    println!("  {}", p.section("Vector Search"));
    row("embed [input]", "Embed content into Qdrant");
    row("query <query>", "Semantic vector search");
    row("retrieve <url-or-path>", "Retrieve stored document");
    row("ask <query>", "Ask over embedded documents");
    row(
        "evaluate <question>",
        "RAG vs baseline + LLM judge (accuracy · relevance · completeness · verdict)",
    );
    row("suggest [focus]", "Suggest new docs URLs to crawl");
    row("sources", "List indexed sources");
    row("domains", "List indexed domains");
    row("stats", "Show vector statistics");
    println!();

    println!("  {}", p.section("Jobs & Diagnostics"));
    row("status", "Show queued job status");
    row("ingest <subcommand>", "Manage shared ingest worker/jobs");
    row(
        "  --max-issues <n>",
        "Max issues per repo (0=all, default 100)",
    );
    row("  --max-prs <n>", "Max PRs per repo (0=all, default 100)");
    row("debug [context]", "LLM-assisted stack troubleshooting");
    row("doctor", "Run local diagnostics");
    row("mcp", "Start MCP stdio or unified HTTP runtime");
    println!();

    println!(
        "  {}",
        p.dim(&format!(
            "→ Run {bin} <command> --help for command-specific flags"
        ))
    );
}

fn print_setup_help() {
    let p = Palette::new();

    println!(
        "{}",
        p.bold(&p.primary("Setup and deploy Axon Docker infrastructure"))
    );
    println!();
    println!(
        "{} {}",
        p.section("Usage:"),
        p.accent("axon setup [COMMAND]")
    );
    println!();
    println!("{}", p.section("Commands:"));

    let cmd_rows: &[(&str, usize, &str)] = &[
        (
            "plugin-hook",
            12,
            "Hook-safe check/repair path for Claude Code SessionStart",
        ),
        (
            "check",
            8,
            "Check local Docker prerequisites without mutating files or services",
        ),
        (
            "repair",
            8,
            "Repair local Axon config, compose assets, and Docker stack",
        ),
        (
            "targets",
            8,
            "List Docker deployment targets from ~/.ssh/config",
        ),
        (
            "deploy",
            8,
            "Deploy the Axon Docker Compose stack to an SSH target",
        ),
        (
            "help",
            8,
            "Print this message or the help of the given subcommand(s)",
        ),
    ];
    for (name, width, desc) in cmd_rows {
        // Width varies per row to match the original column layout exactly.
        match width {
            12 => println!("  {:<12} {}", p.accent(name), desc),
            _ => println!("  {:<8} {}", p.accent(name), desc),
        }
    }
    println!();
    println!("{}", p.section("Options:"));
    println!("  {}  Print help", p.accent("-h, --help"));
}
