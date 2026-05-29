use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;
use url::Url;

use crate::services::types::{
    DiscoveredEndpoint, EndpointKind, EndpointReport, EndpointSourceKind, EndpointVerification,
};

mod classify;
mod scan;
mod script_sources;
use classify::{classify_value, is_noise_value, is_valid_absolute_host, looks_like_endpoint};
use scan::scan_text;
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
    let mut report = new_endpoint_report(base_url, bundles.len());

    let (scripts, script_truncated) =
        discover_script_sources(html, base_url, options.max_scripts.max(1));
    report.scripts_discovered = scripts.len();
    report.truncated |= script_truncated;

    let mut hosts = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut remaining = options.max_scan_bytes.max(1);

    let html_scan = bounded_slice(html, &mut remaining, &mut report);
    scan_text(
        html_scan,
        EndpointSourceKind::InlineScript,
        Some(base_url),
        base.as_ref(),
        base_origin.as_ref(),
        &base_host,
        &mut report,
        &mut hosts,
        &mut seen,
        options,
    );
    scan_html_attributes(
        html_scan,
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
        let bundle_scan = bounded_slice(&bundle.text, &mut remaining, &mut report);
        scan_text(
            bundle_scan,
            EndpointSourceKind::ScriptBundle,
            Some(&bundle.url),
            base.as_ref(),
            base_origin.as_ref(),
            &base_host,
            &mut report,
            &mut hosts,
            &mut seen,
            options,
        );
    }

    report.hosts = hosts.into_iter().collect();
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    report
}

fn new_endpoint_report(base_url: &str, bundles_fetched: usize) -> EndpointReport {
    EndpointReport {
        url: base_url.to_string(),
        endpoints: Vec::new(),
        hosts: Vec::new(),
        scripts_discovered: 0,
        bundles_fetched,
        bundles_scanned: 0,
        truncated: false,
        warnings: Vec::new(),
        elapsed_ms: 0,
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
    if matches!(
        kind,
        EndpointKind::AbsoluteUrl | EndpointKind::Graphql | EndpointKind::Websocket
    ) && !is_valid_absolute_host(&value)
    {
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
        rpc_probe: None,
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
    let base = base_host.to_ascii_lowercase();
    // Exact match or direct subdomain of the full base host.
    if candidate == base || candidate.ends_with(&format!(".{base}")) {
        return true;
    }
    // Registrable-domain comparison: strip leading labels down to the last two
    // (or three for known multi-label TLDs like .co.uk, .com.au) so that
    // www.example.co.uk and api.example.co.uk are both first-party.
    registrable_domain(&candidate) == registrable_domain(&base)
        && !registrable_domain(&candidate).is_empty()
}

/// Returns the registrable domain (last two labels, or last three for
/// known multi-label second-level domains).
fn registrable_domain(host: &str) -> &str {
    const MULTI_LABEL_TLDS: &[&str] = &[
        ".co.uk", ".co.jp", ".co.nz", ".co.za", ".co.in", ".co.kr", ".com.au", ".com.br",
        ".com.mx", ".com.ar", ".com.sg", ".com.hk", ".net.au", ".net.br", ".org.uk", ".org.au",
        ".me.uk", ".ac.uk", ".gov.uk", ".edu.au", ".gov.au",
    ];
    for multi in MULTI_LABEL_TLDS {
        if let Some(prefix) = host.strip_suffix(multi) {
            // e.g. "api.ticketmaster.co.uk" → find the label before ".co.uk"
            if let Some(dot) = prefix.rfind('.') {
                return &host[dot + 1..];
            }
            return host; // already at the registrable domain
        }
    }
    // Default: last two labels
    if let Some(dot) = host.rfind('.')
        && let Some(dot2) = host[..dot].rfind('.')
    {
        return &host[dot2 + 1..];
    }
    host
}

fn endpoint_first_party(value: &str, normalized_url: Option<&str>, base_host: &str) -> bool {
    if value.starts_with('/') && !value.starts_with("//") {
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
