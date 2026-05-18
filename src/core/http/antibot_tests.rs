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
    let body = "<html><script src=\"/cdn-cgi/challenge-platform/...\"></script></html>";
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
