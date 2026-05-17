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
        label: "Ask question",
        subcommand: "ask",
        arg_mode: ArgMode::Single,
        aliases: &["ask", "answer", "rag"],
        description: "Run RAG over the configured collection and synthesize an answer.",
        example: "ask why did OpenClaw rank above Claude docs?",
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
        label: "Ingest target",
        subcommand: "ingest",
        arg_mode: ArgMode::Split,
        aliases: &["ingest", "import", "repo", "youtube", "reddit"],
        description: "Ingest GitHub, Reddit, or YouTube targets into the collection.",
        example: "ingest https://github.com/zed-industries/zed",
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
        matches!(self.subcommand, "scrape" | "crawl" | "map")
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

pub(crate) fn build_axon_args(action: CommandAction, arg: &str) -> Result<Vec<String>, String> {
    // --local forces in-process execution even when AXON_SERVER_URL is set.
    let mut args = vec!["--local".to_string(), action.subcommand.to_string()];
    match action.arg_mode {
        ArgMode::None => {}
        ArgMode::Single => args.push(arg.to_string()),
        ArgMode::Split => args.extend(split_shell_words(arg)?),
    }
    Ok(args)
}

pub(crate) fn display_command_line(args: &[String]) -> String {
    let mut parts = vec!["axon".to_string()];
    parts.extend(args.iter().map(|arg| {
        if arg.contains(char::is_whitespace) {
            format!("{arg:?}")
        } else {
            arg.clone()
        }
    }));
    parts.join(" ")
}

fn split_shell_words(input: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars();
    let mut quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        match ch {
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            '\'' | '"' if quote == Some(ch) => quote = None,
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ch if ch.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            ch => current.push(ch),
        }
    }

    if let Some(quote) = quote {
        return Err(format!("unterminated {quote} quote"));
    }
    if !current.is_empty() {
        words.push(current);
    }
    if words.is_empty() {
        return Err("argument required".to_string());
    }

    Ok(words)
}
