use super::*;

#[test]
fn taxonomy_codes_match_locked_contract() {
    let cases: &[(ServiceTaxonomyError, &str, bool)] = &[
        (
            ServiceTaxonomyError::ChallengeDetected {
                vendor: ChallengeVendor::Cloudflare,
                recoverable: true,
                retry_after: Some(Duration::from_secs(30)),
            },
            "challenge_detected",
            true,
        ),
        (
            ServiceTaxonomyError::ChallengeDetected {
                vendor: ChallengeVendor::DataDome,
                recoverable: false,
                retry_after: None,
            },
            "challenge_detected",
            false,
        ),
        (
            ServiceTaxonomyError::VerticalRateLimited {
                vertical: "github_repo",
                retry_after: Some(Duration::from_secs(60)),
            },
            "vertical_rate_limited",
            true,
        ),
        (
            ServiceTaxonomyError::VerticalAuthMissing {
                vertical: "github_repo",
            },
            "vertical_auth_missing",
            false,
        ),
        (
            ServiceTaxonomyError::VerticalAuthInvalid { vertical: "reddit" },
            "vertical_auth_invalid",
            false,
        ),
        (
            ServiceTaxonomyError::VerticalUnsupportedUrl {
                vertical: "github_repo",
                url: "https://example.com".into(),
            },
            "vertical_unsupported_url",
            false,
        ),
        (
            ServiceTaxonomyError::VerticalTargetNotFound {
                vertical: "github_repo",
                url: "https://github.com/none/none".into(),
            },
            "vertical_target_not_found",
            false,
        ),
        (
            ServiceTaxonomyError::VerticalTargetUnavailable {
                vertical: "github_repo",
                status: 503,
            },
            "vertical_target_unavailable",
            true,
        ),
        (
            ServiceTaxonomyError::VerticalBlockedAntibot {
                vertical: "shopify",
                vendor: ChallengeVendor::Akamai,
            },
            "vertical_blocked_antibot",
            true,
        ),
        (
            ServiceTaxonomyError::StructuredDataMalformed {
                source: "jsonld",
                reason: "trailing comma".into(),
            },
            "structured_data_malformed",
            false,
        ),
        (
            ServiceTaxonomyError::LadderExhausted {
                final_word_count: 12,
            },
            "ladder_exhausted",
            false,
        ),
    ];

    for (err, code, retriable) in cases {
        assert_eq!(err.mcp_code(), *code, "code mismatch for {err:?}");
        assert_eq!(
            err.retriable(),
            *retriable,
            "retriable mismatch for {err:?}"
        );
    }
}

#[test]
fn taxonomy_envelope_shape_is_stable() {
    let err = ServiceTaxonomyError::VerticalRateLimited {
        vertical: "github_repo",
        retry_after: Some(Duration::from_secs(45)),
    };
    let env = err.to_mcp_envelope();
    assert_eq!(env["error"]["code"], "vertical_rate_limited");
    assert_eq!(env["error"]["retriable"], true);
    assert_eq!(env["error"]["source"], "github_repo");
    assert_eq!(env["error"]["details"]["vertical"], "github_repo");
    assert_eq!(env["error"]["details"]["retry_after_secs"], 45);
}

#[test]
fn taxonomy_challenge_envelope_carries_vendor_and_recoverable() {
    let err = ServiceTaxonomyError::ChallengeDetected {
        vendor: ChallengeVendor::Cloudflare,
        recoverable: false,
        retry_after: None,
    };
    let env = err.to_mcp_envelope();
    assert_eq!(env["error"]["code"], "challenge_detected");
    assert_eq!(env["error"]["retriable"], false);
    assert_eq!(env["error"]["source"], "antibot");
    assert_eq!(env["error"]["details"]["vendor"], "cloudflare");
    assert_eq!(env["error"]["details"]["recoverable"], false);
    assert!(env["error"]["details"]["retry_after_secs"].is_null());
}

#[test]
fn taxonomy_display_messages_are_human_readable() {
    let cases: &[(ServiceTaxonomyError, &str)] = &[
        (
            ServiceTaxonomyError::VerticalAuthMissing {
                vertical: "github_repo",
            },
            "github_repo requires credentials",
        ),
        (
            ServiceTaxonomyError::VerticalTargetUnavailable {
                vertical: "reddit",
                status: 502,
            },
            "status=502",
        ),
        (
            ServiceTaxonomyError::StructuredDataMalformed {
                source: "next_data",
                reason: "unexpected eof".into(),
            },
            "next_data structured data malformed: unexpected eof",
        ),
        (
            ServiceTaxonomyError::LadderExhausted {
                final_word_count: 7,
            },
            "final_word_count=7",
        ),
    ];
    for (err, needle) in cases {
        let msg = err.to_string();
        assert!(msg.contains(needle), "expected {needle:?} in {msg:?}");
    }
}

#[test]
fn taxonomy_downcast_works_through_boxed_error_chain() {
    let err: Box<dyn StdError + 'static> =
        Box::new(ServiceTaxonomyError::VerticalAuthMissing {
            vertical: "github_repo",
        });
    let recovered = taxonomy_from_error(err.as_ref()).expect("downcast");
    assert_eq!(recovered.mcp_code(), "vertical_auth_missing");
    assert!(!recovered.retriable());
}

#[test]
fn challenge_vendor_round_trip_strings() {
    assert_eq!(ChallengeVendor::Cloudflare.as_str(), "cloudflare");
    assert_eq!(ChallengeVendor::DataDome.as_str(), "datadome");
    assert_eq!(ChallengeVendor::AwsWaf.as_str(), "aws_waf");
    assert_eq!(ChallengeVendor::HCaptcha.as_str(), "hcaptcha");
    assert_eq!(ChallengeVendor::Akamai.as_str(), "akamai");
    assert_eq!(ChallengeVendor::Other("custom").as_str(), "custom");
}
