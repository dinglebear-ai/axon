use axon::services::types::supported_actions;

const DOC: &str = include_str!("../docs/API-PARITY.md");

fn row_for_cli(command: &str) -> Option<&'static str> {
    let needle = format!("| `{command}` |");
    DOC.lines().find(|line| line.starts_with(&needle))
}

#[test]
fn parity_doc_covers_every_cli_command_kind() {
    let commands = [
        "scrape",
        "crawl",
        "watch",
        "map",
        "extract",
        "search",
        "embed",
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
        "ingest",
        "sessions",
        "research",
        "screenshot",
        "completions",
        "mcp",
        "serve",
        "setup",
        "migrate",
    ];

    for command in commands {
        assert!(
            row_for_cli(command).is_some(),
            "docs/API-PARITY.md is missing CLI command row `{command}`"
        );
    }
}

#[test]
fn parity_doc_lists_all_advertised_http_actions() {
    for action in supported_actions() {
        let needle = format!("`{action}`");
        assert!(
            DOC.contains(&needle) || DOC.contains(&action),
            "docs/API-PARITY.md does not mention advertised HTTP capability `{action}`"
        );
    }
}

#[test]
fn parity_doc_marks_representative_current_http_statuses() {
    let ask = row_for_cli("ask").expect("ask row");
    assert!(ask.contains("`POST /v1/ask`"), "{ask}");
    assert!(ask.contains("Missing"), "{ask}");

    let status = row_for_cli("status").expect("status row");
    assert!(status.contains("`GET /v1/status`"), "{status}");
    assert!(status.contains("Implemented"), "{status}");

    let query = row_for_cli("query").expect("query row");
    assert!(query.contains("`POST /v1/query`"), "{query}");
    assert!(query.contains("Implemented"), "{query}");

    let retrieve = row_for_cli("retrieve").expect("retrieve row");
    assert!(retrieve.contains("`POST /v1/retrieve`"), "{retrieve}");
    assert!(retrieve.contains("Implemented"), "{retrieve}");

    let completions = row_for_cli("completions").expect("completions row");
    assert!(completions.contains("Deferred"), "{completions}");
}
