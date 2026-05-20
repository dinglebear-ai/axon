use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;
use url::Url;

use crate::services::types::{
    DiscoveredEndpoint, EndpointKind, EndpointReport, EndpointSourceKind, EndpointVerification,
};

mod classify;
mod script_sources;
use classify::{
    classify_absolute, classify_relative, classify_value, is_noise_value, looks_like_endpoint,
};
pub use script_sources::discover_script_sources;

pub const DEFAULT_MAX_SCRIPTS: usize = 40;
pub const DEFAULT_MAX_SCAN_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_MAX_ENDPOINTS: usize = 2_000;

#[derive(Debug, Clone)]
pub struct EndpointExtractOptions {
    pub max_scripts: usize,
    pub max_scan_bytes: usize,
    pub unique_only: bool,
    pub max_endpoints: usize,
}

impl Default for EndpointExtractOptions {
    fn default() -> Self {
        Self {
            max_scripts: DEFAULT_MAX_SCRIPTS,
            max_scan_bytes: DEFAULT_MAX_SCAN_BYTES,
            unique_only: true,
            max_endpoints: DEFAULT_MAX_ENDPOINTS,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrefetchedBundle {
    pub url: String,
    pub text: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptSource {
    pub url: String,
    pub first_party: bool,
}

static ATTR_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)\b(?:href|src|action)\s*=\s*["']([^"']+)["']"#).expect("attribute URL regex")
});

static REL_PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?x)
        ["'`]
        (
            /
            [A-Za-z0-9_./~:%+\-?=&]{0,240}
            (?:
                api|graphql|gql|rest|gateway|internal|rpc|json
                |/v[0-9](?:/|$)|v[0-9](?:/|$)
            )
            [A-Za-z0-9_./~:%+\-?=&]{0,240}
        )
        ["'`]
        "#,
    )
    .expect("relative endpoint regex")
});

static ABS_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"https?://[A-Za-z0-9.\-]{1,253}(?::[0-9]{1,5})?(?:/[A-Za-z0-9_./~:%+\-?=&]{0,500})?"#,
    )
    .expect("absolute url regex")
});

static WS_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"wss?://[A-Za-z0-9.\-]{1,253}(?::[0-9]{1,5})?(?:/[A-Za-z0-9_./~:%+\-?=&]{0,500})?"#,
    )
    .expect("websocket url regex")
});

static GRAPHQL_WORD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)["'`]([^"'`]{0,160}(?:graphql|/gql)(?:/[^"'`]*)?)["'`]"#)
        .expect("graphql regex")
});

pub fn extract_endpoints(
    html: &str,
    base_url: &str,
    bundles: &[PrefetchedBundle],
    options: &EndpointExtractOptions,
) -> EndpointReport {
    let started = std::time::Instant::now();
    let base = Url::parse(base_url).ok();
    let base_origin = base.as_ref().and_then(origin_url);
    let base_host = base
        .as_ref()
        .and_then(Url::host_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut report = EndpointReport {
        url: base_url.to_string(),
        endpoints: Vec::new(),
        hosts: Vec::new(),
        scripts_discovered: 0,
        bundles_fetched: bundles.len(),
        bundles_scanned: 0,
        truncated: false,
        warnings: Vec::new(),
        elapsed_ms: 0,
    };

    let (scripts, script_truncated) =
        discover_script_sources(html, base_url, options.max_scripts.max(1));
    report.scripts_discovered = scripts.len();
    report.truncated |= script_truncated;

    let mut hosts = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut remaining = options.max_scan_bytes.max(1);

    scan_text(
        html,
        EndpointSourceKind::InlineScript,
        Some(base_url),
        base.as_ref(),
        base_origin.as_ref(),
        &base_host,
        &mut report,
        &mut hosts,
        &mut seen,
        &mut remaining,
        options,
    );
    scan_html_attributes(
        html,
        base.as_ref(),
        base_origin.as_ref(),
        &base_host,
        &mut report,
        &mut hosts,
        &mut seen,
        options,
    );

    for bundle in bundles.iter().take(options.max_scripts.max(1)) {
        if remaining == 0 || report.endpoints.len() >= options.max_endpoints {
            report.truncated = true;
            break;
        }
        report.bundles_scanned += 1;
        report.truncated |= bundle.truncated;
        scan_text(
            &bundle.text,
            EndpointSourceKind::ScriptBundle,
            Some(&bundle.url),
            base.as_ref(),
            base_origin.as_ref(),
            &base_host,
            &mut report,
            &mut hosts,
            &mut seen,
            &mut remaining,
            options,
        );
    }

    report.hosts = hosts.into_iter().collect();
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    report
}

#[allow(clippy::too_many_arguments)]
fn scan_text(
    text: &str,
    source: EndpointSourceKind,
    source_url: Option<&str>,
    base: Option<&Url>,
    base_origin: Option<&Url>,
    base_host: &str,
    report: &mut EndpointReport,
    hosts: &mut BTreeSet<String>,
    seen: &mut BTreeSet<String>,
    remaining: &mut usize,
    options: &EndpointExtractOptions,
) {
    let slice = bounded_slice(text, remaining, report);
    if slice.is_empty() {
        return;
    }

    for captures in REL_PATH_RE.captures_iter(slice) {
        let Some(value) = captures.get(1).map(|m| m.as_str()) else {
            continue;
        };
        let kind = classify_relative(value);
        push_endpoint(
            value,
            kind,
            source,
            source_url,
            base,
            base_origin,
            base_host,
            report,
            hosts,
            seen,
            options,
        );
    }

    for captures in GRAPHQL_WORD_RE.captures_iter(slice) {
        let Some(value) = captures.get(1).map(|m| m.as_str().trim()) else {
            continue;
        };
        if value.starts_with("http://")
            || value.starts_with("https://")
            || value.starts_with('/')
            || value.starts_with("ws://")
            || value.starts_with("wss://")
        {
            push_endpoint(
                value,
                if value.starts_with("ws://") || value.starts_with("wss://") {
                    EndpointKind::Websocket
                } else {
                    EndpointKind::Graphql
                },
                source,
                source_url,
                base,
                base_origin,
                base_host,
                report,
                hosts,
                seen,
                options,
            );
        }
    }

    for found in WS_URL_RE.find_iter(slice) {
        push_endpoint(
            found.as_str(),
            EndpointKind::Websocket,
            source,
            source_url,
            base,
            base_origin,
            base_host,
            report,
            hosts,
            seen,
            options,
        );
    }

    for found in ABS_URL_RE.find_iter(slice) {
        let value = found.as_str();
        push_endpoint(
            value,
            classify_absolute(value),
            source,
            source_url,
            base,
            base_origin,
            base_host,
            report,
            hosts,
            seen,
            options,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn scan_html_attributes(
    html: &str,
    base: Option<&Url>,
    base_origin: Option<&Url>,
    base_host: &str,
    report: &mut EndpointReport,
    hosts: &mut BTreeSet<String>,
    seen: &mut BTreeSet<String>,
    options: &EndpointExtractOptions,
) {
    for captures in ATTR_URL_RE.captures_iter(html) {
        let Some(value) = captures.get(1).map(|m| m.as_str()) else {
            continue;
        };
        if looks_like_endpoint(value) {
            push_endpoint(
                value,
                classify_value(value),
                EndpointSourceKind::HtmlAttribute,
                base.map(Url::as_str),
                base,
                base_origin,
                base_host,
                report,
                hosts,
                seen,
                options,
            );
        }
    }
}

fn bounded_slice<'a>(text: &'a str, remaining: &mut usize, report: &mut EndpointReport) -> &'a str {
    if *remaining == 0 {
        report.truncated = true;
        return "";
    }
    if text.len() <= *remaining {
        *remaining -= text.len();
        return text;
    }
    report.truncated = true;
    let mut end = *remaining;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    *remaining = 0;
    &text[..end]
}

#[allow(clippy::too_many_arguments)]
fn push_endpoint(
    raw_value: &str,
    kind: EndpointKind,
    source: EndpointSourceKind,
    source_url: Option<&str>,
    base: Option<&Url>,
    base_origin: Option<&Url>,
    base_host: &str,
    report: &mut EndpointReport,
    hosts: &mut BTreeSet<String>,
    seen: &mut BTreeSet<String>,
    options: &EndpointExtractOptions,
) {
    if report.endpoints.len() >= options.max_endpoints {
        report.truncated = true;
        return;
    }
    let value = clean_endpoint_value(raw_value);
    if value.is_empty() || is_noise_value(&value) {
        return;
    }
    let normalized_url = normalize_endpoint(&value, kind, base, base_origin);
    let first_party = endpoint_first_party(&value, normalized_url.as_deref(), base_host);
    if let Some(host) = normalized_url.as_deref().and_then(url_host) {
        hosts.insert(host);
    } else if let Some(host) = absolute_url_host(&value) {
        hosts.insert(host);
    }

    let dedupe_key = if options.unique_only {
        normalized_url.clone().unwrap_or_else(|| value.clone())
    } else {
        format!(
            "{}|{}|{}",
            normalized_url.clone().unwrap_or_else(|| value.clone()),
            source.as_str(),
            source_url.unwrap_or_default()
        )
    };
    if !seen.insert(dedupe_key) {
        return;
    }

    report.endpoints.push(DiscoveredEndpoint {
        value,
        normalized_url,
        kind,
        first_party,
        source,
        source_url: source_url.map(ToString::to_string),
        verified: None::<EndpointVerification>,
    });
}

fn normalize_endpoint(
    value: &str,
    kind: EndpointKind,
    base: Option<&Url>,
    base_origin: Option<&Url>,
) -> Option<String> {
    if matches!(kind, EndpointKind::Websocket) {
        return Url::parse(value).ok().map(|url| url.to_string());
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return Url::parse(value).ok().map(|url| url.to_string());
    }
    if value.starts_with('/')
        && let Some(origin) = base_origin
    {
        return origin.join(value).ok().map(|url| url.to_string());
    }
    base.and_then(|url| url.join(value).ok())
        .map(|url| url.to_string())
}

fn origin_url(url: &Url) -> Option<Url> {
    let mut origin = url.clone();
    origin.set_path("/");
    origin.set_query(None);
    origin.set_fragment(None);
    Some(origin)
}

fn host_is_first_party(candidate: Option<&str>, base_host: &str) -> bool {
    let Some(candidate) = candidate else {
        return true;
    };
    let candidate = candidate.to_ascii_lowercase();
    candidate == base_host || candidate.ends_with(&format!(".{base_host}"))
}

fn endpoint_first_party(value: &str, normalized_url: Option<&str>, base_host: &str) -> bool {
    if value.starts_with('/') {
        return true;
    }
    let Some(url) = normalized_url else {
        return true;
    };
    host_is_first_party(url_host(url).as_deref(), base_host)
}

fn url_host(value: &str) -> Option<String> {
    Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
}

fn absolute_url_host(value: &str) -> Option<String> {
    if value.starts_with("http://") || value.starts_with("https://") || value.starts_with("ws://") {
        url_host(value)
    } else {
        None
    }
}

fn clean_endpoint_value(value: &str) -> String {
    value
        .trim()
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | ')' | '(' | ',' | ';'))
        .trim_end_matches(['\\'])
        .to_string()
}

pub fn endpoint_host_counts(endpoints: &[DiscoveredEndpoint]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for endpoint in endpoints {
        if let Some(host) = endpoint
            .normalized_url
            .as_deref()
            .and_then(url_host)
            .or_else(|| absolute_url_host(&endpoint.value))
        {
            *counts.entry(host).or_insert(0) += 1;
        }
    }
    counts
}

#[cfg(test)]
#[path = "endpoints_tests.rs"]
mod tests;
