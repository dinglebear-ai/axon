//! Antibot challenge-page detection (axon_rust-gc59).
//!
//! Ports webclaw `webclaw-fetch/src/cloud.rs:396-461`. Detects 8 WAF / antibot
//! signatures with per-vendor byte-length gates to avoid false positives on
//! legitimate pages that happen to embed widgets (e.g. a docs page with a
//! Turnstile-protected signup form). Pure logic — no I/O.
//!
//! ## When to invoke
//! The crawl/scrape path runs `detect_challenge` BEFORE the thin-page filter.
//! A CF challenge HTML body is 200–500 chars of "checking your browser" —
//! today axon's `--drop-thin-markdown` filter silently drops it. Wiring this
//! upstream is what unlocks `ServiceTaxonomyError::ChallengeDetected` and the
//! Akamai cookie-warmup retry (cookie warmup helper lives in the sibling
//! `cookie_warmup.rs` module).
//!
//! ## Page-size gates
//! Per-vendor gates from webclaw — pages larger than the cap likely embed the
//! widget legitimately (e.g. a Turnstile-protected form on a content page)
//! and should NOT be flagged as challenges.
//!
//! ## Byte budget
//! All scans are bounded by `max_scan_bytes` (`cfg.antibot_max_body_scan_bytes`,
//! default 150 KiB from zehr). Pages larger than that get a single one-pass
//! head-scan; we never lowercase a multi-MB body.

use crate::services::error::ChallengeVendor;

/// What a single `detect_challenge` call returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeDetection {
    /// WAF / antibot vendor identified.
    pub vendor: ChallengeVendor,
    /// `true` when this pattern is one of the documented Akamai
    /// signatures that the homepage-warmup retry can recover from.
    /// Other vendors return `false` — there's no in-process recovery,
    /// the caller should propagate `ServiceTaxonomyError::ChallengeDetected`.
    pub akamai_warmup_recoverable: bool,
}

/// Cloudflare body size gate for the Turnstile-widget heuristic. Pages
/// larger than this likely embed Turnstile legitimately (signup forms).
const CF_TURNSTILE_BODY_CAP: usize = 100_000;
/// AWS WAF interstitial gate. The shell page is tiny — Trustpilot's full
/// challenge HTML is < 10 KiB.
const AWS_WAF_INTERSTITIAL_BODY_CAP: usize = 10_000;
/// hCaptcha gate. Pages larger than this likely have hCaptcha embedded
/// on a content page (e.g. account signup) rather than as a full block.
const HCAPTCHA_BODY_CAP: usize = 50_000;
/// Cap for the head-scan window when the body exceeds `max_scan_bytes`.
/// 50 KiB is generous enough to contain all 8 fingerprints (typical
/// challenge pages are < 5 KiB).
const HEAD_SCAN_BYTES: usize = 50_000;

/// Detect an antibot challenge in `html_body` + `response_headers`.
///
/// `header_lookup` returns the response header value (case-insensitive,
/// first match) for the given header name. Most callers will wrap a
/// `reqwest::header::HeaderMap` with a small closure; the indirection
/// avoids leaking reqwest types into pure-logic core code.
///
/// `max_scan_bytes` is `cfg.antibot_max_body_scan_bytes` (zehr,
/// default 150 KiB). Bodies larger than the cap get a single head-scan
/// of `HEAD_SCAN_BYTES` rather than a full `.to_lowercase()`.
///
/// Returns `None` when no challenge fingerprint matches. The caller
/// should propagate the `ChallengeDetection` to
/// `ServiceTaxonomyError::ChallengeDetected` (a9l6 taxonomy).
pub fn detect_challenge<F>(
    html_body: &str,
    header_lookup: F,
    max_scan_bytes: usize,
) -> Option<ChallengeDetection>
where
    F: Fn(&str) -> Option<String>,
{
    let body_len = html_body.len();

    // Build the scan window once. For huge bodies, scan only the head —
    // every documented signature appears in the first few KiB of the
    // page. The full `.to_lowercase()` of a multi-MB body would be a
    // 10 ms+ heat-spike per page.
    let scan_window: String = if body_len <= max_scan_bytes {
        html_body.to_lowercase()
    } else {
        let cap = HEAD_SCAN_BYTES.min(body_len);
        // Walk back to the nearest char boundary so the &str slice is valid.
        let mut bound = cap;
        while bound > 0 && !html_body.is_char_boundary(bound) {
            bound -= 1;
        }
        html_body[..bound].to_lowercase()
    };

    // ── Akamai Bot Manager: bazadebezolkohpepadr is the canonical token ──
    //
    // This is the strongest signal documented. When it fires, the homepage
    // cookie-warmup retry is the recovery path (see cookie_warmup.rs).
    if scan_window.contains("bazadebezolkohpepadr") {
        return Some(ChallengeDetection {
            vendor: ChallengeVendor::Akamai,
            akamai_warmup_recoverable: true,
        });
    }

    // ── Cloudflare: chl_opt / challenge-platform are CF's own tokens ─────
    if scan_window.contains("_cf_chl_opt") || scan_window.contains("challenge-platform") {
        return Some(ChallengeDetection {
            vendor: ChallengeVendor::Cloudflare,
            akamai_warmup_recoverable: false,
        });
    }

    // ── Cloudflare interstitial: phrase + spinner co-occurrence ──────────
    //
    // Both must appear — legitimate pages may say "checking your browser"
    // in copy.
    let has_just_a_moment =
        scan_window.contains("just a moment") || scan_window.contains("checking your browser");
    if has_just_a_moment && scan_window.contains("cf-spinner") {
        return Some(ChallengeDetection {
            vendor: ChallengeVendor::Cloudflare,
            akamai_warmup_recoverable: false,
        });
    }

    // ── Cloudflare Turnstile widget — gated by body size ─────────────────
    //
    // Smaller bodies embedding cf-turnstile are likely challenge pages;
    // larger bodies are likely content pages with Turnstile on a form.
    if body_len < CF_TURNSTILE_BODY_CAP
        && (scan_window.contains("cf-turnstile")
            || scan_window.contains("challenges.cloudflare.com/turnstile"))
    {
        return Some(ChallengeDetection {
            vendor: ChallengeVendor::Cloudflare,
            akamai_warmup_recoverable: false,
        });
    }

    // ── DataDome ─────────────────────────────────────────────────────────
    if scan_window.contains("geo.captcha-delivery.com")
        || scan_window.contains("captcha-delivery.com/captcha")
    {
        return Some(ChallengeDetection {
            vendor: ChallengeVendor::DataDome,
            akamai_warmup_recoverable: false,
        });
    }

    // ── AWS WAF captcha ──────────────────────────────────────────────────
    if scan_window.contains("awswaf-captcha") || scan_window.contains("aws-waf-client-browser") {
        return Some(ChallengeDetection {
            vendor: ChallengeVendor::AwsWaf,
            akamai_warmup_recoverable: false,
        });
    }

    // ── AWS WAF interstitial (Trustpilot style) — body < 10 KiB ─────────
    if body_len < AWS_WAF_INTERSTITIAL_BODY_CAP
        && scan_window.contains("interstitial-spinner")
        && scan_window.contains("verifying your connection")
    {
        return Some(ChallengeDetection {
            vendor: ChallengeVendor::AwsWaf,
            akamai_warmup_recoverable: false,
        });
    }

    // ── hCaptcha — body < 50 KiB, both fingerprints required ────────────
    if body_len < HCAPTCHA_BODY_CAP
        && scan_window.contains("hcaptcha.com")
        && scan_window.contains("h-captcha")
    {
        return Some(ChallengeDetection {
            vendor: ChallengeVendor::HCaptcha,
            akamai_warmup_recoverable: false,
        });
    }

    // ── Cloudflare via headers ───────────────────────────────────────────
    //
    // Catches lightweight challenge pages where the body fingerprints
    // aren't present but CF response headers expose a mitigation.
    let cf_header_present = header_lookup("cf-ray").is_some()
        || header_lookup("cf-mitigated").is_some_and(|v| !v.eq_ignore_ascii_case("none"));
    if cf_header_present && has_just_a_moment {
        return Some(ChallengeDetection {
            vendor: ChallengeVendor::Cloudflare,
            akamai_warmup_recoverable: false,
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_headers(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn detects_akamai_via_token() {
        let body =
            "<html><body><script>bazadebezolkohpepadr = ...</script></body></html>".to_string();
        let d = detect_challenge(&body, no_headers, 150_000).unwrap();
        assert_eq!(d.vendor, ChallengeVendor::Akamai);
        assert!(
            d.akamai_warmup_recoverable,
            "Akamai must be marked recoverable"
        );
    }

    #[test]
    fn detects_cloudflare_chl_opt() {
        let body = "<html>...var _cf_chl_opt = {};...</html>";
        let d = detect_challenge(body, no_headers, 150_000).unwrap();
        assert_eq!(d.vendor, ChallengeVendor::Cloudflare);
        assert!(!d.akamai_warmup_recoverable);
    }

    #[test]
    fn detects_cloudflare_challenge_platform() {
        let body = "<html><script src=\"/cdn-cgi/challenge-platform/...\"></script></html>";
        let d = detect_challenge(body, no_headers, 150_000).unwrap();
        assert_eq!(d.vendor, ChallengeVendor::Cloudflare);
    }

    #[test]
    fn detects_cloudflare_interstitial() {
        let body =
            "<html><body><h1>Just a moment</h1><div class=\"cf-spinner\"></div></body></html>";
        let d = detect_challenge(body, no_headers, 150_000).unwrap();
        assert_eq!(d.vendor, ChallengeVendor::Cloudflare);
    }

    #[test]
    fn interstitial_requires_both_phrase_and_spinner() {
        // "checking your browser" without cf-spinner = NOT a challenge
        // (might be legitimate help-page copy).
        let body =
            "<html><p>Welcome — we are checking your browser version for compatibility.</p></html>";
        assert!(detect_challenge(body, no_headers, 150_000).is_none());
    }

    #[test]
    fn detects_cf_turnstile_under_size_gate() {
        let body = "<html><div class=\"cf-turnstile\"></div></html>"; // tiny
        let d = detect_challenge(body, no_headers, 150_000).unwrap();
        assert_eq!(d.vendor, ChallengeVendor::Cloudflare);
    }

    #[test]
    fn cf_turnstile_skipped_when_body_too_large() {
        // A long content page that embeds Turnstile for an inline form
        // must NOT be flagged.
        let filler: String = "<p>genuine content content content</p>".repeat(3000);
        let body = format!("<html>{filler}<div class=\"cf-turnstile\"></div></html>");
        assert!(detect_challenge(&body, no_headers, 200_000).is_none());
    }

    #[test]
    fn detects_datadome() {
        let body = "<html>...geo.captcha-delivery.com...</html>";
        let d = detect_challenge(body, no_headers, 150_000).unwrap();
        assert_eq!(d.vendor, ChallengeVendor::DataDome);
    }

    #[test]
    fn detects_aws_waf_captcha() {
        let body = "<html><div id=\"awswaf-captcha\"></div></html>";
        let d = detect_challenge(body, no_headers, 150_000).unwrap();
        assert_eq!(d.vendor, ChallengeVendor::AwsWaf);
    }

    #[test]
    fn detects_aws_waf_interstitial_when_small() {
        let body = "<html><body><div class=\"interstitial-spinner\"></div><p>Verifying your connection...</p></body></html>";
        let d = detect_challenge(body, no_headers, 150_000).unwrap();
        assert_eq!(d.vendor, ChallengeVendor::AwsWaf);
    }

    #[test]
    fn aws_waf_interstitial_requires_small_body() {
        let filler: String = "<p>real content</p>".repeat(2000); // way over 10 KiB
        let body = format!(
            "<html>{filler}<div class=\"interstitial-spinner\"></div><p>Verifying your connection...</p></html>"
        );
        assert!(detect_challenge(&body, no_headers, 200_000).is_none());
    }

    #[test]
    fn detects_hcaptcha_when_small() {
        let body = "<html>hcaptcha.com <div class=\"h-captcha\"></div></html>";
        let d = detect_challenge(body, no_headers, 150_000).unwrap();
        assert_eq!(d.vendor, ChallengeVendor::HCaptcha);
    }

    #[test]
    fn hcaptcha_skipped_when_body_too_large() {
        let filler: String = "<p>article body content</p>".repeat(4000);
        let body = format!("<html>{filler}hcaptcha.com <div class=\"h-captcha\"></div></html>");
        assert!(detect_challenge(&body, no_headers, 200_000).is_none());
    }

    #[test]
    fn detects_cloudflare_via_header_with_body_phrase() {
        let body = "<html><body>Just a moment...</body></html>";
        let h = |name: &str| -> Option<String> {
            if name.eq_ignore_ascii_case("cf-ray") {
                Some("8a1234abcd-AMS".into())
            } else {
                None
            }
        };
        let d = detect_challenge(body, h, 150_000).unwrap();
        assert_eq!(d.vendor, ChallengeVendor::Cloudflare);
    }

    #[test]
    fn cf_mitigated_none_does_not_trigger() {
        let body = "<html><body>Just a moment</body></html>";
        let h = |name: &str| -> Option<String> {
            if name.eq_ignore_ascii_case("cf-mitigated") {
                Some("none".into())
            } else {
                None
            }
        };
        // cf-mitigated: none means CF did NOT challenge this request.
        // Without cf-ray, "just a moment" alone is ambiguous text.
        assert!(detect_challenge(body, h, 150_000).is_none());
    }

    #[test]
    fn no_match_returns_none() {
        let body = "<html><body><h1>Welcome</h1><p>Regular content.</p></body></html>";
        assert!(detect_challenge(body, no_headers, 150_000).is_none());
    }

    #[test]
    fn huge_body_scans_head_window() {
        // 5 MiB body — must still detect the fingerprint near the top
        // without lowercasing the whole thing.
        let mut body = "<html><script>var _cf_chl_opt = {};</script>".to_string();
        body.push_str(&"x".repeat(5 * 1024 * 1024));
        body.push_str("</html>");
        let d = detect_challenge(&body, no_headers, 150_000).unwrap();
        assert_eq!(d.vendor, ChallengeVendor::Cloudflare);
    }

    #[test]
    fn huge_body_fingerprint_past_head_is_missed_by_design() {
        // Documented limitation: a fingerprint > HEAD_SCAN_BYTES into a
        // body larger than max_scan_bytes will be missed. Typical
        // challenge pages put the fingerprint in the first few KiB, so
        // this is a deliberate cost trade-off — not a regression.
        let mut body = "<html>".to_string();
        body.push_str(&"x".repeat(5 * 1024 * 1024));
        body.push_str("<script>var _cf_chl_opt = {};</script></html>");
        assert!(detect_challenge(&body, no_headers, 150_000).is_none());
    }
}
