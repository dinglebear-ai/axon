use super::ScriptSource;
use regex::Regex;
use std::collections::BTreeSet;
use std::sync::LazyLock;
use url::Url;

static SCRIPT_SRC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<script\b[^>]*\bsrc\s*=\s*(?:"([^"]+)"|'([^']+)'|([^'"\s>]+))[^>]*>"#)
        .expect("script src regex")
});

pub fn discover_script_sources(
    html: &str,
    base_url: &str,
    max_scripts: usize,
) -> (Vec<ScriptSource>, bool) {
    let base = match Url::parse(base_url) {
        Ok(url) => url,
        Err(_) => return (Vec::new(), false),
    };
    let base_host = base.host_str().unwrap_or_default().to_ascii_lowercase();
    let mut scripts = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut truncated = false;

    for captures in SCRIPT_SRC_RE.captures_iter(html) {
        let Some(src) = captures
            .get(1)
            .or_else(|| captures.get(2))
            .or_else(|| captures.get(3))
            .map(|m| m.as_str())
        else {
            continue;
        };
        let Some(resolved) = resolve_url(&base, src) else {
            continue;
        };
        if resolved.scheme() != "http" && resolved.scheme() != "https" {
            continue;
        }
        let url = resolved.to_string();
        if !seen.insert(url.clone()) {
            continue;
        }
        if scripts.len() >= max_scripts {
            truncated = true;
            break;
        }
        scripts.push(ScriptSource {
            first_party: host_is_first_party(resolved.host_str(), &base_host),
            url,
        });
    }

    (scripts, truncated)
}

fn resolve_url(base: &Url, value: &str) -> Option<Url> {
    Url::parse(value).ok().or_else(|| base.join(value).ok())
}

fn host_is_first_party(candidate: Option<&str>, base_host: &str) -> bool {
    let Some(candidate) = candidate else {
        return true;
    };
    let candidate = candidate.to_ascii_lowercase();
    candidate == base_host || candidate.ends_with(&format!(".{base_host}"))
}
