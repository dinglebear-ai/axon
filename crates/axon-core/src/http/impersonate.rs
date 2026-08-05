//! Browser TLS + HTTP/2 fingerprint impersonation via `wreq` (BoringSSL).
//!
//! This is a baseline acquisition capability in every Axon binary. `wreq`
//! builds BoringSSL and therefore requires cmake, clang, perl, and go.
//!
//! ## Why this exists
//!
//! Some WAFs (Akamai Bot Manager, notably) fingerprint the TLS ClientHello and
//! the HTTP/2 SETTINGS/priority frames, not the request headers. `rustls`
//! cannot express the knobs that fingerprint is computed from — cipher order,
//! extension order, GREASE placement, ALPS, H2 SETTINGS order, pseudo-header
//! order — so no amount of header tuning on the shared `reqwest` client gets
//! past them.
//!
//! Measured 2026-07-28 against four Akamai-fronted SC county sites
//! (dorchestercountysc.gov, cityofrockhill.com, richlandcountysc.gov,
//! northaugustasc.gov):
//!
//! | client                                          | result |
//! |-------------------------------------------------|--------|
//! | reqwest+rustls, browser UA                       | 403    |
//! | reqwest+rustls, Chrome UA + full browser headers | 403    |
//! | wreq Chrome profile (this module)                | 200    |
//!
//! Profile values are ported from webclaw
//! (`crates/webclaw-fetch/src/tls.rs`, AGPL-3.0) — the cipher/sigalg/curve
//! strings and the extension/SETTINGS orderings are wire-format constants, not
//! original logic. See bead `axon_rust-wf4s`.
//!
//! ## SSRF
//!
//! `wreq` gets its own DNS resolver that reuses [`validate_resolved_ips`], so
//! the connect-time SSRF guard that [`super::ssrf::SsrfBlockingResolver`]
//! provides for `reqwest` is preserved here rather than bypassed.

use std::time::Duration;

use wreq::http2::{
    Http2Options, PseudoId, PseudoOrder, SettingId, SettingsOrder, StreamDependency, StreamId,
};
use wreq::tls::{AlpnProtocol, AlpsProtocol, ExtensionType, TlsOptions, TlsVersion};
use wreq::{Client, Emulation};

use super::error::HttpError;
use super::normalize::normalize_url;
use super::ssrf::{validate_resolved_ips, validate_url};

/// Chrome's TLS cipher list, in Chrome's exact wire order.
const CHROME_CIPHERS: &str = "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256:TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256:TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256:TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384:TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384:TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256:TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256:TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA:TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA:TLS_RSA_WITH_AES_128_GCM_SHA256:TLS_RSA_WITH_AES_256_GCM_SHA384:TLS_RSA_WITH_AES_128_CBC_SHA:TLS_RSA_WITH_AES_256_CBC_SHA";

/// Chrome's signature algorithms.
const CHROME_SIGALGS: &str = "ecdsa_secp256r1_sha256:rsa_pss_rsae_sha256:rsa_pkcs1_sha256:ecdsa_secp384r1_sha384:rsa_pss_rsae_sha384:rsa_pss_rsae_sha512:rsa_pkcs1_sha512";

/// Chrome's supported groups, post-quantum `X25519MLKEM768` first — a strong
/// modern-Chrome tell that WAFs check for.
const CHROME_CURVES: &str = "X25519MLKEM768:X25519:P-256:P-384";

/// Chrome request headers in wire order. Order matters: the header sequence is
/// itself part of what an HTTP/2 fingerprint hashes.
const CHROME_HEADERS: &[(&str, &str)] = &[
    (
        "sec-ch-ua",
        r#""Google Chrome";v="145", "Chromium";v="145", "Not/A)Brand";v="24""#,
    ),
    ("sec-ch-ua-mobile", "?0"),
    ("sec-ch-ua-platform", "\"Windows\""),
    ("upgrade-insecure-requests", "1"),
    (
        "user-agent",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36",
    ),
    (
        "accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7",
    ),
    ("sec-fetch-site", "none"),
    ("sec-fetch-mode", "navigate"),
    ("sec-fetch-user", "?1"),
    ("sec-fetch-dest", "document"),
    ("accept-encoding", "gzip, deflate, br"),
    ("accept-language", "en-US,en;q=0.9"),
    ("priority", "u=0, i"),
];

/// Chrome's TLS extension order.
///
/// `permute_extensions` is deliberately OFF so this fixed order is what goes on
/// the wire. Real Chrome permutes extensions per handshake, but WAF rulesets
/// are built against the canonical order and some reject permuted ones.
fn chrome_extensions() -> Vec<ExtensionType> {
    vec![
        ExtensionType::CERTIFICATE_TIMESTAMP,
        ExtensionType::STATUS_REQUEST,
        ExtensionType::SESSION_TICKET,
        ExtensionType::KEY_SHARE,
        ExtensionType::SUPPORTED_GROUPS,
        ExtensionType::PSK_KEY_EXCHANGE_MODES,
        ExtensionType::EC_POINT_FORMATS,
        ExtensionType::CERT_COMPRESSION,
        ExtensionType::APPLICATION_SETTINGS_NEW,
        ExtensionType::SUPPORTED_VERSIONS,
        ExtensionType::SIGNATURE_ALGORITHMS,
        ExtensionType::SERVER_NAME,
        ExtensionType::APPLICATION_LAYER_PROTOCOL_NEGOTIATION,
        ExtensionType::ENCRYPTED_CLIENT_HELLO,
        ExtensionType::RENEGOTIATE,
        ExtensionType::EXTENDED_MASTER_SECRET,
    ]
}

fn chrome_tls() -> TlsOptions {
    TlsOptions::builder()
        .cipher_list(CHROME_CIPHERS)
        .sigalgs_list(CHROME_SIGALGS)
        .curves_list(CHROME_CURVES)
        .min_tls_version(TlsVersion::TLS_1_2)
        .max_tls_version(TlsVersion::TLS_1_3)
        .grease_enabled(true)
        .permute_extensions(false)
        .extension_permutation(chrome_extensions())
        .enable_ech_grease(true)
        .pre_shared_key(true)
        .enable_ocsp_stapling(true)
        .enable_signed_cert_timestamps(true)
        .alpn_protocols([AlpnProtocol::HTTP2, AlpnProtocol::HTTP1])
        .alps_protocols([AlpsProtocol::HTTP2])
        .alps_use_new_codepoint(true)
        .aes_hw_override(true)
        .build()
}

fn chrome_h2() -> Http2Options {
    // MAX_CONCURRENT_STREAMS is deliberately absent: real Chrome omits it, and
    // its presence reads as a bot signal. The HEADERS stream-dependency frame
    // (weight 256, exclusive) is the field that most directly moves the
    // Akamai HTTP/2 fingerprint hash.
    Http2Options::builder()
        .initial_window_size(6_291_456)
        .initial_connection_window_size(15_728_640)
        .max_header_list_size(262_144)
        .header_table_size(65_536)
        .enable_push(false)
        .settings_order(
            SettingsOrder::builder()
                .extend([
                    SettingId::HeaderTableSize,
                    SettingId::EnablePush,
                    SettingId::InitialWindowSize,
                    SettingId::MaxHeaderListSize,
                ])
                .build(),
        )
        .headers_pseudo_order(
            PseudoOrder::builder()
                .extend([
                    PseudoId::Method,
                    PseudoId::Authority,
                    PseudoId::Scheme,
                    PseudoId::Path,
                ])
                .build(),
        )
        .headers_stream_dependency(StreamDependency::new(StreamId::zero(), 255, true))
        .build()
}

/// SSRF-guarded DNS resolver for `wreq`.
///
/// Mirrors [`super::ssrf::SsrfBlockingResolver`]: resolve, then reject the
/// whole response if any address falls in a blocked range, so the impersonating
/// client cannot be used to reach internal hosts.
#[derive(Clone, Default)]
struct SsrfWreqResolver;

impl wreq::dns::Resolve for SsrfWreqResolver {
    fn resolve(&self, name: wreq::dns::Name) -> wreq::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            type DnsError = Box<dyn std::error::Error + Send + Sync>;

            let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host(format!("{host}:0"))
                .await
                .map_err(|e| Box::new(e) as DnsError)?
                .collect();

            // Partition rather than short-circuit so the denial can be audited
            // with the offending addresses, exactly as SsrfBlockingResolver does.
            let (allowed, blocked): (Vec<_>, Vec<_>) = addrs
                .into_iter()
                .partition(|addr| validate_resolved_ips(&host, [addr.ip()]).is_ok());

            if !blocked.is_empty() {
                crate::http::ssrf::record_resolver_denial(
                    &host,
                    blocked.iter().map(|addr| addr.ip()).collect(),
                );
                let err: DnsError = Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("SSRF: DNS response for '{host}' contains blocked IP ranges"),
                ));
                return Err(err);
            }
            let addrs = allowed;

            if addrs.is_empty() {
                let err: DnsError = Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("host '{host}' did not resolve to any address"),
                ));
                return Err(err);
            }

            Ok(Box::new(addrs.into_iter()) as wreq::dns::Addrs)
        })
    }
}

/// Maximum redirect hops.
///
/// Uses the same `>` comparison as both `reqwest` and `wreq` apply internally
/// (`previous` includes the initial URI, so `> N` permits N redirects), so this
/// client and the shared `reqwest` client follow the same number of hops.
const MAX_REDIRECT_HOPS: usize = 10;

/// SSRF-revalidating redirect policy.
///
/// **This is the load-bearing SSRF control on this path, not the resolver.**
/// `wreq::redirect::Policy::limited` is purely count-based, and wreq's
/// per-hop validation checks only the URI *scheme*. Worse, wreq's connector
/// skips DNS resolution entirely when the host is already an IP literal
/// ("skip resolving the dns and start connecting right away"), so
/// [`SsrfWreqResolver`] is never consulted for `http://169.254.169.254/` or
/// `http://127.0.0.1:6333/`.
///
/// Without this policy an attacker who controls a site axon was asked to fetch
/// can answer the escalated request with `302 Location: http://169.254.169.254/…`
/// and read internal endpoints. The shared `reqwest` client avoids this only
/// because it revalidates every hop; this mirrors that.
fn ssrf_revalidating_redirect_policy() -> wreq::redirect::Policy {
    wreq::redirect::Policy::custom(|attempt| {
        if attempt.previous.len() > MAX_REDIRECT_HOPS {
            return attempt.error(std::io::Error::other(format!(
                "too many redirects (>{MAX_REDIRECT_HOPS})"
            )));
        }
        let next = attempt.uri.to_string();
        match validate_url(&next) {
            Ok(()) => attempt.follow(),
            Err(e) => attempt.error(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("SSRF: impersonated redirect to blocked URL rejected: {e}"),
            )),
        }
    })
}

fn build_impersonating_client() -> Result<Client, String> {
    let mut headers = wreq::header::HeaderMap::with_capacity(CHROME_HEADERS.len());
    for (name, value) in CHROME_HEADERS {
        let (Ok(n), Ok(v)) = (
            wreq::header::HeaderName::from_bytes(name.as_bytes()),
            wreq::header::HeaderValue::from_str(value),
        ) else {
            return Err(format!("invalid built-in Chrome header: {name}"));
        };
        headers.insert(n, v);
    }

    let emulation = Emulation::builder()
        .tls_options(chrome_tls())
        .http2_options(chrome_h2())
        .headers(headers)
        .build();

    Client::builder()
        .emulation(emulation)
        // A new client (and therefore a new jar) is built for each escalated
        // acquisition. wreq's jar does not validate a `Set-Cookie` `Domain=`
        // attribute against the responding host and has no public-suffix list,
        // so sharing a jar across arbitrary fetched hosts would let one host
        // inject cookies that ride along on an unrelated later request.
        .cookie_provider(std::sync::Arc::new(wreq::cookie::Jar::default()))
        .redirect(ssrf_revalidating_redirect_policy())
        .dns_resolver(SsrfWreqResolver)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
}

/// Browser-impersonating HTTP client with request-scoped cookie state.
///
/// The client is intentionally not cached: these requests target arbitrary
/// user-supplied hosts, and `wreq`'s cookie jar cannot safely be shared among
/// them. This path is only used after a bot-wall classification, so the small
/// client-construction cost does not affect ordinary acquisition requests.
pub fn impersonating_client() -> Result<Client, HttpError> {
    build_impersonating_client().map_err(HttpError::ImpersonationInit)
}

/// A response from the browser-impersonating client.
///
/// Carries the observed status and post-redirect URL rather than synthesising
/// them, so callers can classify and attribute the result accurately.
#[derive(Debug, Clone)]
pub struct ImpersonatedResponse {
    pub body: String,
    pub status: u16,
    pub final_url: String,
}

/// Fetch `url` as HTML through the browser-impersonating client.
///
/// Applies the same parse-time SSRF validation as [`super::fetch_html`]; the
/// connect-time guard is enforced by [`SsrfWreqResolver`].
pub async fn fetch_html_impersonated(url: &str) -> Result<ImpersonatedResponse, HttpError> {
    let normalized = normalize_url(url);
    validate_url(&normalized)?;
    let client = impersonating_client()?;

    let response = client
        .get(normalized.as_ref())
        .send()
        .await
        .map_err(|e| HttpError::ImpersonationRequest(e.to_string()))?;

    let status = response.status().as_u16();
    // Capture the POST-redirect URL. Losing it would attribute bytes fetched from
    // a redirect target to the original request URL — which, on a path that can
    // be redirected, destroys the provenance needed to notice an SSRF.
    let final_url = response.uri().to_string();
    let body = response
        .text()
        .await
        .map_err(|e| HttpError::ImpersonationRequest(e.to_string()))?;

    Ok(ImpersonatedResponse {
        body,
        status,
        final_url,
    })
}

#[cfg(test)]
#[path = "impersonate_tests.rs"]
mod tests;
