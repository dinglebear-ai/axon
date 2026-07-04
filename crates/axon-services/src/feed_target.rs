//! Feed-target detection + normalization for `axon source <input>`.
//!
//! Ported from the legacy `axon-ingest::classify` feed heuristics so transports
//! (CLI/MCP/web) can route on feed-ness without depending on the `axon-ingest`
//! crate (slated for removal). Two pure functions:
//!
//! * [`is_feed_target`] — does the input look like an RSS/Atom/RDF feed? Honors
//!   an explicit `rss:`/`feed:`/`atom:` prefix or a feed-shaped URL (`.rss`/
//!   `.atom`/`.rdf` extension, a `feed`/`rss`/`atom` path segment or common feed
//!   filename, or a feed-selecting query parameter).
//! * [`normalize_feed_target`] — strip any `rss:`/`feed:`/`atom:` prefix down to
//!   the real https URL that the acquire helper will fetch.
//!
//! The feed adapter (`axon_adapters::feed`) intentionally exposes no detection
//! or normalization — it only reads a prepared `feed_path` option — so this
//! logic lives here.

use url::Url;

/// Explicit feed prefixes that force feed classification regardless of URL
/// shape. Scheme is added when the prefixed remainder omits one.
const FEED_PREFIXES: [&str; 3] = ["rss:", "feed:", "atom:"];

/// True when `input` should route to the feed acquisition path.
///
/// Pure — string parsing only, no I/O — so routing is testable without a data
/// plane. An explicit `rss:`/`feed:`/`atom:` prefix (with a non-empty
/// remainder) always counts; otherwise the input must parse as an http/https
/// URL whose shape looks like a feed.
pub fn is_feed_target(input: &str) -> bool {
    if let Some(rest) = strip_feed_prefix(input) {
        return !rest.trim().is_empty();
    }
    looks_like_feed_url(input)
}

/// Strip any `rss:`/`feed:`/`atom:` prefix from `input` and return the real
/// https URL the acquire helper should fetch.
///
/// A prefixed remainder that already carries a scheme (`rss:https://…`) is kept
/// as-is; a bare host/path remainder (`rss:example.com/feed`) is upgraded to
/// `https://`. Inputs without a feed prefix are returned trimmed and unchanged.
pub fn normalize_feed_target(input: &str) -> String {
    match strip_feed_prefix(input) {
        Some(rest) => {
            let rest = rest.trim();
            if rest.contains("://") {
                rest.to_string()
            } else {
                format!("https://{rest}")
            }
        }
        None => input.trim().to_string(),
    }
}

/// If `input` carries a `rss:`/`feed:`/`atom:` prefix, return the remainder
/// (untrimmed); otherwise `None`.
fn strip_feed_prefix(input: &str) -> Option<&str> {
    let trimmed = input.trim_start();
    FEED_PREFIXES
        .iter()
        .find_map(|prefix| trimmed.strip_prefix(prefix))
}

/// Conservative heuristic: does this URL look like an RSS/Atom/RDF feed?
///
/// Ported verbatim from `axon-ingest::classify::looks_like_feed_url` to keep
/// routing behavior identical to the legacy `ingest` classifier.
fn looks_like_feed_url(input: &str) -> bool {
    let Ok(url) = Url::parse(input) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let path = url.path().to_ascii_lowercase();
    let has_feed_ext = path.ends_with(".rss") || path.ends_with(".atom") || path.ends_with(".rdf");
    let last_segment = path.rsplit('/').next().unwrap_or("");
    let common_feed_file = matches!(
        last_segment,
        "feed.xml" | "rss.xml" | "atom.xml" | "index.xml" | "feed" | "rss" | "atom"
    );
    let feed_segment = path
        .split('/')
        .any(|seg| matches!(seg, "feed" | "feeds" | "rss" | "atom"));
    // Match a real feed query parameter (e.g. `?feed=rss2`, `?format=atom`),
    // not any query that merely mentions a feed word. A bare `feed` key counts;
    // a feed-shaped value (`rss`/`atom`/…) counts only under a format-selecting
    // key, so `?feedback=1` and `?category=atom` are NOT treated as feeds.
    let feed_query = url.query_pairs().any(|(k, v)| {
        let k = k.to_ascii_lowercase();
        k == "feed"
            || ((k == "format" || k == "type" || k == "output")
                && matches!(
                    v.to_ascii_lowercase().as_str(),
                    "rss" | "rss2" | "atom" | "rdf" | "feed"
                ))
    });
    has_feed_ext || common_feed_file || feed_segment || feed_query
}

#[cfg(test)]
#[path = "feed_target_tests.rs"]
mod tests;
