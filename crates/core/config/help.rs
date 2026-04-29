use std::env;
use std::process;

pub(super) fn maybe_print_top_level_help_and_exit() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 2 && matches!(args[1].as_str(), "-h" | "--help" | "help") {
        print_top_level_help();
        process::exit(0);
    }
}

fn print_top_level_help() {
    let colors_enabled = env::var("AXON_NO_COLOR").is_err();
    let colorize = |code: &str, text: &str| {
        if colors_enabled {
            format!("{code}{text}\x1b[0m")
        } else {
            text.to_string()
        }
    };
    let bold = |text: &str| {
        if colors_enabled {
            format!("\x1b[1m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    };
    let dim = |text: &str| colorize("\x1b[2m", text);

    let primary = "\x1b[38;2;244;143;177m";
    let accent = "\x1b[38;2;144;202;249m";

    let title = bold(&colorize(primary, "AXON CLI"));
    let divider = colorize(primary, "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    let section = |name: &str| bold(&colorize(primary, name));
    let cmd = |name: &str| colorize(accent, name);
    let bin_name = env::args()
        .next()
        .and_then(|p| {
            std::path::Path::new(&p)
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "axon".to_string());

    println!("  {title}");
    println!("  {divider}");
    println!(
        "  Version {}  |  {}",
        env!("CARGO_PKG_VERSION"),
        dim("Spider-powered web and local RAG CLI")
    );
    println!();
    println!("  {}", section("Usage"));
    println!("  {}", cmd(&format!("[{bin_name} [options] [command]]")));
    println!();
    println!("  {}", section("Quick Start"));
    println!(
        "  {}",
        dim(&format!(
            "{bin_name} scrape https://example.com --wait true --embed false"
        ))
    );
    println!(
        "  {}",
        dim(&format!(
            "{bin_name} crawl https://docs.rs/spider --wait false"
        ))
    );
    println!(
        "  {}",
        dim(&format!(
            "{bin_name} query \"embedding pipeline\" --collection cortex"
        ))
    );
    println!();
    println!("  {}", section("Global Options"));
    println!("  {:<28} {}", cmd("-h, --help"), dim("display help"));
    println!(
        "  {:<28} {}",
        cmd("--wait <bool>"),
        dim("run synchronously (default false)")
    );
    println!(
        "  {:<28} {}",
        cmd("--collection <name>"),
        dim("vector collection (default cortex)")
    );
    println!(
        "  {:<28} {}",
        cmd("--embed <bool>"),
        dim("run embedding where applicable")
    );
    println!(
        "  {:<28} {}",
        cmd("--cache <bool>"),
        dim("reuse prior crawl artifacts when possible")
    );
    println!(
        "  {:<28} {}",
        cmd("--cache-skip-browser <bool>"),
        dim("force HTTP crawl path when cache flow is enabled")
    );
    println!(
        "  {:<28} {}",
        cmd("--max-pages <n>"),
        dim("crawl page limit (0 = uncapped)")
    );
    println!(
        "  {:<28} {}",
        cmd("--url-glob <pattern[,..]>"),
        dim("expand URL seeds via brace globs (e.g. {1..10}, {a,b})")
    );
    println!(
        "  {:<28} {}",
        cmd("--cron-every-seconds <n>"),
        dim("repeat command every n seconds")
    );
    println!(
        "  {:<28} {}",
        cmd("--cron-max-runs <n>"),
        dim("stop cron loop after n runs")
    );
    println!("  {:<28} {}", cmd("--max-depth <n>"), dim("crawl depth"));
    println!(
        "  {:<28} {}",
        cmd("--output-dir <dir>"),
        dim("output directory")
    );
    println!();
    println!("  {}", section("Core Web Operations"));
    println!("  {:<28} {}", cmd("scrape [url]"), dim("Scrape a URL"));
    println!("  {:<28} {}", cmd("crawl [url]"), dim("Crawl a website"));
    println!(
        "  {:<28} {}",
        cmd("map [url]"),
        dim("Map URLs on a website")
    );
    println!(
        "  {:<28} {}",
        cmd("search <query>"),
        dim("Search web results")
    );
    println!(
        "  {:<28} {}",
        cmd("extract [urls...]"),
        dim("Extract structured data")
    );
    println!();
    println!("  {}", section("Vector Search"));
    println!(
        "  {:<28} {}",
        cmd("embed [input]"),
        dim("Embed content into Qdrant")
    );
    println!(
        "  {:<28} {}",
        cmd("query <query>"),
        dim("Semantic vector search")
    );
    println!(
        "  {:<28} {}",
        cmd("retrieve <url-or-path>"),
        dim("Retrieve stored document")
    );
    println!(
        "  {:<28} {}",
        cmd("ask <query>"),
        dim("Ask over embedded documents")
    );
    println!(
        "  {:<28} {}",
        cmd("evaluate <question>"),
        dim("RAG vs baseline + LLM judge (accuracy · relevance · completeness · verdict)")
    );
    println!(
        "  {:<28} {}",
        cmd("suggest [focus]"),
        dim("Suggest new docs URLs to crawl")
    );
    println!("  {:<28} {}", cmd("sources"), dim("List indexed sources"));
    println!("  {:<28} {}", cmd("domains"), dim("List indexed domains"));
    println!("  {:<28} {}", cmd("stats"), dim("Show vector statistics"));
    println!();
    println!("  {}", section("Jobs & Diagnostics"));
    println!("  {:<28} {}", cmd("status"), dim("Show queued job status"));
    println!(
        "  {:<28} {}",
        cmd("ingest <subcommand>"),
        dim("Manage shared ingest worker/jobs")
    );
    println!(
        "  {:<28} {}",
        cmd("  --max-issues <n>"),
        dim("Max issues per repo (0=all, default 100)")
    );
    println!(
        "  {:<28} {}",
        cmd("  --max-prs <n>"),
        dim("Max PRs per repo (0=all, default 100)")
    );
    println!(
        "  {:<28} {}",
        cmd("debug [context]"),
        dim("LLM-assisted stack troubleshooting")
    );
    println!("  {:<28} {}", cmd("doctor"), dim("Run local diagnostics"));
    println!(
        "  {:<28} {}",
        cmd("mcp"),
        dim("Start MCP HTTP server runtime")
    );
    println!();
    println!(
        "  {}",
        dim(&format!(
            "→ Run {bin_name} <command> --help for command-specific flags"
        ))
    );
}
