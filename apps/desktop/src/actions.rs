#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;

#[derive(Clone, Copy)]
pub(crate) struct CommandAction {
    pub(crate) label: &'static str,
    /// `axon` subcommand to invoke. `arg_mode` controls how the typed suffix is
    /// converted into argv entries.
    pub(crate) subcommand: &'static str,
    pub(crate) arg_mode: ArgMode,
    pub(crate) aliases: &'static [&'static str],
    pub(crate) description: &'static str,
    pub(crate) example: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgMode {
    None,
    OptionalSingle,
    Single,
    Split,
}

pub(crate) const ACTIONS: &[CommandAction] = &[
    CommandAction {
        label: "Scrape URL",
        subcommand: "scrape",
        arg_mode: ArgMode::Split,
        aliases: &["scrape", "fetch", "page", "url"],
        description: "Fetch one page, convert it to markdown, and optionally embed it.",
        example: "scrape https://docs.rs/serde",
    },
    CommandAction {
        label: "Crawl URL",
        subcommand: "crawl",
        arg_mode: ArgMode::Split,
        aliases: &["crawl", "site", "docs"],
        description: "Queue a site crawl from a start URL with the current crawl settings.",
        example: "crawl https://docs.anthropic.com",
    },
    CommandAction {
        label: "Map URL",
        subcommand: "map",
        arg_mode: ArgMode::Split,
        aliases: &["map", "links", "discover"],
        description: "Discover URLs without scraping or embedding page content.",
        example: "map https://code.claude.com/docs",
    },
    CommandAction {
        label: "Summarize URL",
        subcommand: "summarize",
        arg_mode: ArgMode::Split,
        aliases: &["summarize", "summary", "brief"],
        description: "Scrape one or more URLs and synthesize a concise summary.",
        example: "summarize https://docs.rs/serde",
    },
    CommandAction {
        label: "Ask question",
        subcommand: "ask",
        arg_mode: ArgMode::Single,
        aliases: &["ask", "answer", "rag"],
        description: "Run RAG over the configured collection and synthesize an answer.",
        example: "ask why did OpenClaw rank above Claude docs?",
    },
    CommandAction {
        label: "Chat with LLM",
        subcommand: "chat",
        arg_mode: ArgMode::Single,
        aliases: &["chat", "llm", "talk"],
        description: "Chat directly with the configured LLM without RAG retrieval.",
        example: "chat explain this error simply",
    },
    CommandAction {
        label: "Query knowledge base",
        subcommand: "query",
        arg_mode: ArgMode::Single,
        aliases: &["query", "vector", "semantic"],
        description: "Search indexed chunks semantically and return ranked source snippets.",
        example: "query gpui menu rendering",
    },
    CommandAction {
        label: "Retrieve document",
        subcommand: "retrieve",
        arg_mode: ArgMode::Split,
        aliases: &["retrieve", "chunks", "document"],
        description: "Fetch stored document content for an indexed URL.",
        example: "retrieve https://docs.rs/serde",
    },
    CommandAction {
        label: "Suggest URLs",
        subcommand: "suggest",
        arg_mode: ArgMode::OptionalSingle,
        aliases: &["suggest", "recommend", "discover-more"],
        description: "Suggest additional documentation URLs worth crawling.",
        example: "suggest gpui",
    },
    CommandAction {
        label: "Evaluate answer",
        subcommand: "evaluate",
        arg_mode: ArgMode::Single,
        aliases: &["evaluate", "eval", "judge"],
        description: "Compare RAG and baseline answers with an independent LLM judge.",
        example: "evaluate how does gpui menu routing work?",
    },
    CommandAction {
        label: "Search the web",
        subcommand: "search",
        arg_mode: ArgMode::Single,
        aliases: &["search", "web"],
        description: "Search the web and enqueue crawls for useful results.",
        example: "search claude code plugins",
    },
    CommandAction {
        label: "Research the web",
        subcommand: "research",
        arg_mode: ArgMode::Single,
        aliases: &["research", "deepsearch"],
        description: "Run web research with LLM synthesis.",
        example: "research qdrant hybrid search tuning",
    },
    CommandAction {
        label: "Embed input",
        subcommand: "embed",
        arg_mode: ArgMode::Single,
        aliases: &["embed", "index", "vectorize"],
        description: "Embed a URL, file, directory, or text input into the collection.",
        example: "embed https://docs.rs/serde",
    },
    CommandAction {
        label: "Extract data",
        subcommand: "extract",
        arg_mode: ArgMode::Split,
        aliases: &["extract", "structured", "parse"],
        description: "Queue structured extraction for one or more URLs.",
        example: "extract https://example.com/pricing",
    },
    CommandAction {
        label: "Ingest target",
        subcommand: "ingest",
        arg_mode: ArgMode::Split,
        aliases: &["ingest", "import", "repo", "youtube", "reddit"],
        description: "Ingest GitHub, Reddit, or YouTube targets into the collection.",
        example: "ingest https://github.com/zed-industries/zed",
    },
    CommandAction {
        label: "Settings",
        subcommand: "settings",
        arg_mode: ArgMode::None,
        aliases: &["settings", "config", "preferences"],
        description: "Configure Axon URLs, secrets, and config.toml options.",
        example: "settings",
    },
    CommandAction {
        label: "Reset ask conversation",
        // Sentinel — handled internally in `Palette::submit`, never shelled
        // out. We still give it an `ArgMode` so the standard "no argument
        // required" path applies.
        subcommand: "ask-reset",
        arg_mode: ArgMode::None,
        aliases: &["ask-reset", "reset-ask", "new-chat", "fresh-ask"],
        description: "Forget the live ask conversation so the next question starts fresh.",
        example: "ask-reset",
    },
    CommandAction {
        label: "Job status",
        subcommand: "status",
        arg_mode: ArgMode::None,
        aliases: &["status", "jobs", "queue"],
        description: "Show the async job queue and recent worker state.",
        example: "status",
    },
    CommandAction {
        label: "List sources",
        subcommand: "sources",
        arg_mode: ArgMode::None,
        aliases: &["sources", "urls", "indexed"],
        description: "List indexed source URLs in the configured collection.",
        example: "sources",
    },
    CommandAction {
        label: "List domains",
        subcommand: "domains",
        arg_mode: ArgMode::None,
        aliases: &["domains", "sites", "facets"],
        description: "Show indexed domains and vector counts.",
        example: "domains",
    },
    CommandAction {
        label: "Collection stats",
        subcommand: "stats",
        arg_mode: ArgMode::None,
        aliases: &["stats", "collection", "qdrant"],
        description: "Show vector collection statistics.",
        example: "stats",
    },
    CommandAction {
        label: "Doctor",
        subcommand: "doctor",
        arg_mode: ArgMode::None,
        aliases: &["doctor", "health", "check"],
        description: "Check Qdrant, TEI, and LLM connectivity.",
        example: "doctor",
    },
];

impl CommandAction {
    pub(crate) fn accepts_direct_url(self) -> bool {
        matches!(
            self.subcommand,
            "scrape" | "crawl" | "map" | "summarize" | "retrieve" | "embed" | "extract"
        )
    }
}

pub(crate) fn action_invoked_by(action: CommandAction, token: &str) -> bool {
    let token = token.trim();
    !token.is_empty()
        && (action.subcommand.eq_ignore_ascii_case(token)
            || action
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(token)))
}

pub(crate) fn action_matches(action: CommandAction, input: &str) -> bool {
    let needle = input.trim();
    if needle.is_empty() {
        return true;
    }

    contains_ignore_ascii_case(action.subcommand, needle)
        || contains_ignore_ascii_case(action.label, needle)
        || action
            .aliases
            .iter()
            .any(|alias| contains_ignore_ascii_case(alias, needle))
}

/// Case-insensitive (ASCII) substring search. Avoids the per-call
/// `String` allocation that `to_lowercase()` would introduce — this runs
/// on the palette render hot path for every action, every frame the
/// search field is dirty. All ACTIONS labels/aliases are ASCII.
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let hay = haystack.as_bytes();
    let ndl = needle.as_bytes();
    if ndl.len() > hay.len() {
        return false;
    }
    hay.windows(ndl.len())
        .any(|window| window.eq_ignore_ascii_case(ndl))
}

pub(crate) fn looks_like_url(input: &str) -> bool {
    let input = input.trim();
    input.starts_with("http://") || input.starts_with("https://")
}
