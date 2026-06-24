use super::classify::{classify_absolute, classify_relative};
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn scan_text(
    text: &str,
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
    if text.is_empty() {
        return;
    }
    scan_relative_paths(
        text,
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
    scan_graphql_words(
        text,
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
    scan_websocket_urls(
        text,
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
    scan_absolute_urls(
        text,
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

#[allow(clippy::too_many_arguments)]
fn scan_relative_paths(
    text: &str,
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
    for captures in REL_PATH_RE.captures_iter(text) {
        let Some(value) = captures.get(1).map(|m| m.as_str()) else {
            continue;
        };
        push_endpoint(
            value,
            classify_relative(value),
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
fn scan_graphql_words(
    text: &str,
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
    for captures in GRAPHQL_WORD_RE.captures_iter(text) {
        let Some(value) = captures.get(1).map(|m| m.as_str().trim()) else {
            continue;
        };
        let lower = value.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || value.starts_with('/')
            || lower.starts_with("ws://")
            || lower.starts_with("wss://")
        {
            push_endpoint(
                value,
                if lower.starts_with("ws://") || lower.starts_with("wss://") {
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
}

#[allow(clippy::too_many_arguments)]
fn scan_websocket_urls(
    text: &str,
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
    for found in WS_URL_RE.find_iter(text) {
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
}

#[allow(clippy::too_many_arguments)]
fn scan_absolute_urls(
    text: &str,
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
    for found in ABS_URL_RE.find_iter(text) {
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
