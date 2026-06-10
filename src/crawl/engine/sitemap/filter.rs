//! URL scope and content-type filtering shared by sitemap and llms.txt discovery.

use crate::core::config::Config;
use crate::crawl::engine::{canonicalize_url_for_dedupe, is_excluded_url_path};
use spider::url::Url;

/// Returns `true` if `lastmod` (ISO 8601 date or datetime string) falls within the last
/// `since_days` days. Unknown / unparseable dates are treated as recent (not filtered out).
pub(super) fn lastmod_is_recent(lastmod: &str, since_days: u32) -> bool {
    use chrono::{NaiveDate, Utc};
    let cutoff = Utc::now().date_naive() - chrono::Duration::days(i64::from(since_days));
    // Accept both "YYYY-MM-DD" and "YYYY-MM-DDTHH:MM:SSZ" by taking the first 10 chars.
    let prefix = lastmod.get(..10).unwrap_or(lastmod);
    match NaiveDate::parse_from_str(prefix, "%Y-%m-%d") {
        Ok(date) => date >= cutoff,
        Err(_) => true, // unparseable → include (don't silently drop)
    }
}

/// Returns the canonicalized URL if `loc` is in scope for a crawl/discovery rooted at
/// `start_host`/`start_path`, else `None`. Shared by sitemap and llms.txt discovery.
/// Same-host by default; honors `cfg.include_subdomains` and `cfg.exclude_path_prefix`.
pub(crate) fn loc_in_scope(
    cfg: &Config,
    loc: &str,
    start_host: &str,
    start_path: &str,
    scoped_to_root: bool,
) -> Option<String> {
    let u = Url::parse(loc).ok()?;
    let h = u.host_str()?;
    let in_scope = if cfg.include_subdomains {
        h == start_host
            || h.strip_suffix(start_host)
                .is_some_and(|rest| rest.ends_with('.'))
    } else {
        h == start_host
    };
    if !in_scope || is_excluded_url_path(loc, &cfg.exclude_path_prefix) {
        return None;
    }
    if !scoped_to_root {
        let p = u.path();
        let exact = p == start_path;
        // Avoid allocating a temporary String for the nested check.
        let nested = p.starts_with(start_path) && p.as_bytes().get(start_path.len()) == Some(&b'/');
        if !exact && !nested {
            return None;
        }
    }
    canonicalize_url_for_dedupe(loc)
}

/// Raw markdown/text targets (e.g. llms.txt-listed `.md` docs) must skip the HTML→markdown
/// transform — `to_markdown(main_content:true)` would strip them to nothing and drop them as thin.
pub(crate) fn is_already_markdown(url: &str) -> bool {
    // Compare only the path, ignoring query/fragment.
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".txt")
}
