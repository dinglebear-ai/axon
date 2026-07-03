use super::{COMMAND_SECTIONS, command_rows};
use std::collections::HashSet;

#[test]
fn top_level_help_commands_come_from_clap_surface() {
    let names: Vec<String> = command_rows().into_iter().map(|(name, _)| name).collect();

    for expected in [
        "watch",
        "monitor",
        "map",
        "extract",
        "search",
        "research",
        "debug",
        "doctor",
        "query",
        "retrieve",
        "ask",
        "evaluate",
        "train",
        "suggest",
        "sources",
        "domains",
        "stats",
        "status",
        "dedupe",
        "purge",
        "fresh",
        "memory",
        "sessions",
        "sync",
        "screenshot",
        "completions",
        "serve",
        "setup",
        "mcp",
        "migrate",
        "config",
    ] {
        assert!(names.iter().any(|name| name == expected), "{expected}");
    }

    // The pipeline-unification clean break (#298 P10) removed these commands.
    for removed in ["scrape", "crawl", "embed", "ingest", "code-search"] {
        assert!(
            !names.iter().any(|name| name == removed),
            "removed command still present in clap surface: {removed}"
        );
    }
}

#[test]
fn curated_command_sections_cover_current_clap_surface() {
    let names: HashSet<String> = command_rows().into_iter().map(|(name, _)| name).collect();
    let categorized: HashSet<&str> = COMMAND_SECTIONS
        .iter()
        .flat_map(|(_, commands)| commands.iter().copied())
        .collect();

    for name in names {
        assert!(categorized.contains(name.as_str()), "{name}");
    }
}
