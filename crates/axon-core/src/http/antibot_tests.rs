use super::*;

fn no_headers(_: &str) -> Option<String> {
    None
}

#[test]
fn detects_akamai_via_token() {
    let body = "<html><body><script>bazadebezolkohpepadr = ...</script></body></html>".to_string();
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
    // Was `/cdn-cgi/challenge-platform/...` — a placeholder that matched the
    // old bare-substring rule. That rule also matched Cloudflare's passive
    // `/scripts/jsd/` beacon on ordinary 200 pages, so the test was asserting
    // the false-positive behaviour. Uses the real orchestrate path now; the
    // beacon case is pinned by `cloudflare_passive_jsd_beacon_is_not_a_challenge`.
    let body = "<html><script src=\"/cdn-cgi/challenge-platform/h/b/orchestrate/chl_page/v1\"></script></html>";
    let d = detect_challenge(body, no_headers, 150_000).unwrap();
    assert_eq!(d.vendor, ChallengeVendor::Cloudflare);
}

#[test]
fn detects_cloudflare_interstitial() {
    let body = "<html><body><h1>Just a moment</h1><div class=\"cf-spinner\"></div></body></html>";
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

/// Cloudflare injects this beacon into ORDINARY served pages, not challenges.
/// Verbatim from townofpageland.com (HTTP 200, 68 KB, 43 links) on 2026-07-29.
const CF_PASSIVE_JSD_BEACON: &str = r#"<html><head><title>Welcome To Pageland, South Carolina</title>
<script>var a=document.createElement('script');
a.src='/cdn-cgi/challenge-platform/scripts/jsd/main.js';
document.getElementsByTagName('head')[0].appendChild(a);</script></head>
<body><a href="/government">Government</a><a href="/contact">Contact</a></body></html>"#;

#[test]
fn cloudflare_passive_jsd_beacon_is_not_a_challenge() {
    // Regression: the bare substring `challenge-platform` used to match here,
    // so every Cloudflare-fronted site was reported as a bot wall. Pageland
    // mapped 0 URLs because of it, and the impersonated retry saw the same
    // beacon and "confirmed" the false wall.
    assert_eq!(
        detect_challenge(CF_PASSIVE_JSD_BEACON, |_| None, 150 * 1024),
        None,
        "a passive JS-detection beacon on a normal 200 page is not a challenge"
    );
}

#[test]
fn cloudflare_real_challenge_still_detected() {
    // The challenge-options blob only appears on a genuine interstitial.
    let chl_opt = r#"<html><head><script>window._cf_chl_opt={cvId:"3"};</script></head></html>"#;
    let d = detect_challenge(chl_opt, |_| None, 150 * 1024)
        .expect("_cf_chl_opt is a definitive Cloudflare challenge signal");
    assert_eq!(d.vendor, ChallengeVendor::Cloudflare);

    // As does the orchestrate path, which is distinct from /scripts/jsd/.
    let orchestrate = r#"<html><body><script src="/cdn-cgi/challenge-platform/h/b/orchestrate/chl_page/v1"></script></body></html>"#;
    let d = detect_challenge(orchestrate, |_| None, 150 * 1024)
        .expect("the orchestrate path is a real challenge");
    assert_eq!(d.vendor, ChallengeVendor::Cloudflare);
}
