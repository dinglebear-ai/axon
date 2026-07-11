use super::*;
use axon_api::source::SecurityPolicyDecision;

#[test]
fn denied_private_ip_url_produces_deny_audit_record() {
    let (result, event) = validate_url_with_audit("http://192.168.1.1/", 0, false);

    assert!(result.is_err(), "private IP must be denied");
    assert_eq!(event.kind, SecurityAuditEventKind::SsrfDenied);

    let detail = event.ssrf.expect("ssrf detail present on denial");
    assert_eq!(detail.policy_decision, SecurityPolicyDecision::Deny);
    assert_eq!(detail.resolved_ip_class, ResolvedIpClass::Private);
    assert_eq!(detail.requested_url, "http://192.168.1.1/");
    assert_eq!(detail.redirect_chain_index, 0);
    assert!(!detail.headers_redacted);

    // Reason must never echo the raw requested URL (it could carry userinfo
    // credentials or tokens); it should reference the blocked IP instead.
    assert!(!event.reason.contains("http://192.168.1.1/"));
    assert!(event.reason.contains("192.168.1.1"));
}

#[test]
fn denied_link_local_metadata_ip_is_classified_link_local() {
    let (result, event) =
        validate_url_with_audit("http://169.254.169.254/latest/meta-data", 0, true);

    assert!(result.is_err());
    let detail = event.ssrf.expect("ssrf detail present");
    assert_eq!(detail.resolved_ip_class, ResolvedIpClass::LinkLocal);
    assert_eq!(detail.policy_decision, SecurityPolicyDecision::Deny);
    assert!(detail.headers_redacted, "headers_present must round-trip");
}

#[test]
fn allowed_public_url_records_allow_decision() {
    let (result, event) = validate_url_with_audit("https://example.com/page", 2, false);

    assert!(result.is_ok());
    let detail = event.ssrf.expect("ssrf detail present");
    assert_eq!(detail.policy_decision, SecurityPolicyDecision::Allow);
    assert_eq!(detail.redirect_chain_index, 2);
    // Hostname URLs have no literal IP without DNS resolution.
    assert_eq!(detail.resolved_ip_class, ResolvedIpClass::NotResolved);
}

#[test]
fn hostname_literal_ip_is_classified_without_dns() {
    let (_, event) = validate_url_with_audit("http://8.8.8.8/", 0, false);
    let detail = event.ssrf.expect("ssrf detail present");
    assert_eq!(detail.resolved_ip_class, ResolvedIpClass::Public);
}

#[test]
fn resolved_ips_with_audit_denies_and_classifies_blocked_ip() {
    let ips = vec!["10.0.0.5".parse().unwrap(), "10.0.0.6".parse().unwrap()];
    let (result, event) = validate_resolved_ips_with_audit(
        "https://internal.example.com/",
        "internal.example.com",
        ips,
        1,
        false,
    );

    assert!(result.is_err());
    let detail = event.ssrf.expect("ssrf detail present");
    assert_eq!(detail.resolved_ip_class, ResolvedIpClass::Private);
    assert_eq!(detail.policy_decision, SecurityPolicyDecision::Deny);
    assert_eq!(detail.redirect_chain_index, 1);
}

#[test]
fn resolved_ips_with_audit_allows_public_ip() {
    let ips = vec!["93.184.216.34".parse().unwrap()];
    let (result, event) =
        validate_resolved_ips_with_audit("https://example.com/", "example.com", ips, 0, false);

    assert!(result.is_ok());
    let detail = event.ssrf.expect("ssrf detail present");
    assert_eq!(detail.resolved_ip_class, ResolvedIpClass::Public);
    assert_eq!(detail.policy_decision, SecurityPolicyDecision::Allow);
}

#[test]
fn classify_ip_covers_all_blocked_ranges() {
    assert_eq!(
        classify_ip("127.0.0.1".parse().unwrap()),
        ResolvedIpClass::Loopback
    );
    assert_eq!(
        classify_ip("0.0.0.0".parse().unwrap()),
        ResolvedIpClass::Unspecified
    );
    assert_eq!(
        classify_ip("169.254.1.1".parse().unwrap()),
        ResolvedIpClass::LinkLocal
    );
    assert_eq!(
        classify_ip("10.1.2.3".parse().unwrap()),
        ResolvedIpClass::Private
    );
    assert_eq!(
        classify_ip("172.16.0.1".parse().unwrap()),
        ResolvedIpClass::Private
    );
    assert_eq!(
        classify_ip("192.168.0.1".parse().unwrap()),
        ResolvedIpClass::Private
    );
    assert_eq!(
        classify_ip("1.1.1.1".parse().unwrap()),
        ResolvedIpClass::Public
    );
    assert_eq!(
        classify_ip("::1".parse().unwrap()),
        ResolvedIpClass::Loopback
    );
    assert_eq!(
        classify_ip("fe80::1".parse().unwrap()),
        ResolvedIpClass::LinkLocal
    );
    assert_eq!(
        classify_ip("fc00::1".parse().unwrap()),
        ResolvedIpClass::UniqueLocal
    );
    assert_eq!(
        classify_ip("::ffff:192.168.1.1".parse().unwrap()),
        ResolvedIpClass::Private,
        "IPv4-mapped IPv6 must recurse into the v4 classification"
    );
}

#[test]
fn event_kind_and_policy_ref_are_stamped() {
    let (_, event) = validate_url_with_audit("http://192.168.1.1/", 0, false);
    assert_eq!(event.policy_id.as_deref(), Some("axon-core-ssrf"));
    assert!(event.policy_version.is_some());
}
