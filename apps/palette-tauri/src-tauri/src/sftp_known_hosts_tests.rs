use super::*;

#[test]
fn evaluate_host_key_flags_a_new_host_for_trust_prompt_not_auto_accept() {
    let store = KnownHostsStore(Vec::new());
    let decision = evaluate_host_key(
        &store,
        "example.com",
        22,
        "ssh-ed25519",
        "AAAA...fingerprint",
    );
    match decision {
        HostKeyDecision::NewHostNeedsPrompt(entry) => {
            assert_eq!(entry.host, "example.com");
            assert_eq!(entry.fingerprint, "AAAA...fingerprint");
        }
        HostKeyDecision::TrustedMatch => {
            panic!(
                "a host with no prior pinned entry must never resolve to TrustedMatch — \
                 this would mean host-key verification silently accepts any key on first \
                 connect with no trust decision at all, which is exactly the always-accept \
                 regression this test exists to catch"
            );
        }
        HostKeyDecision::Mismatch { .. } => {
            panic!("no prior entry exists, Mismatch is impossible here")
        }
    }
}

#[test]
fn evaluate_host_key_matches_a_pinned_fingerprint() {
    let mut store = KnownHostsStore(Vec::new());
    pin_host_key(
        &mut store,
        KnownHostEntry {
            host: "example.com".to_string(),
            port: 22,
            key_type: "ssh-ed25519".to_string(),
            fingerprint: "AAAA...fingerprint".to_string(),
            first_seen_unix: 0,
        },
    );
    let decision = evaluate_host_key(
        &store,
        "example.com",
        22,
        "ssh-ed25519",
        "AAAA...fingerprint",
    );
    assert!(matches!(decision, HostKeyDecision::TrustedMatch));
}

#[test]
fn evaluate_host_key_hard_fails_on_fingerprint_mismatch_never_silently_repins() {
    let mut store = KnownHostsStore(Vec::new());
    pin_host_key(
        &mut store,
        KnownHostEntry {
            host: "example.com".to_string(),
            port: 22,
            key_type: "ssh-ed25519".to_string(),
            fingerprint: "AAAA...original".to_string(),
            first_seen_unix: 0,
        },
    );
    let decision = evaluate_host_key(&store, "example.com", 22, "ssh-ed25519", "BBBB...different");
    match decision {
        HostKeyDecision::Mismatch {
            pinned,
            seen_fingerprint,
        } => {
            assert_eq!(pinned.fingerprint, "AAAA...original");
            assert_eq!(seen_fingerprint, "BBBB...different");
        }
        other => panic!(
            "expected Mismatch, got {other:?} — a fingerprint change must never be silently re-pinned"
        ),
    }
    // The store itself must be unmodified by evaluation alone — pinning only
    // happens via an explicit pin_host_key call from a user-confirmed prompt.
    assert_eq!(store.0[0].fingerprint, "AAAA...original");
}

#[test]
fn revoke_host_key_removes_the_matching_entry() {
    let mut store = KnownHostsStore(Vec::new());
    pin_host_key(
        &mut store,
        KnownHostEntry {
            host: "example.com".to_string(),
            port: 22,
            key_type: "ssh-ed25519".to_string(),
            fingerprint: "AAAA...fingerprint".to_string(),
            first_seen_unix: 0,
        },
    );
    revoke_host_key(&mut store, "example.com", 22);
    assert!(store.0.is_empty());
}

#[test]
fn evaluate_host_key_normalizes_hostname_case_and_trailing_dot() {
    let mut store = KnownHostsStore(Vec::new());
    pin_host_key(
        &mut store,
        KnownHostEntry {
            host: "example.com".to_string(),
            port: 22,
            key_type: "ssh-ed25519".to_string(),
            fingerprint: "AAAA...fingerprint".to_string(),
            first_seen_unix: 0,
        },
    );
    // Different case and a trailing dot are the same pinned identity — must
    // not each trigger their own "new host" TOFU prompt.
    assert!(matches!(
        evaluate_host_key(
            &store,
            "EXAMPLE.COM",
            22,
            "ssh-ed25519",
            "AAAA...fingerprint"
        ),
        HostKeyDecision::TrustedMatch
    ));
    assert!(matches!(
        evaluate_host_key(
            &store,
            "example.com.",
            22,
            "ssh-ed25519",
            "AAAA...fingerprint"
        ),
        HostKeyDecision::TrustedMatch
    ));
}

#[test]
fn revoke_host_key_normalizes_hostname_before_matching() {
    let mut store = KnownHostsStore(Vec::new());
    pin_host_key(
        &mut store,
        KnownHostEntry {
            host: "Example.COM".to_string(),
            port: 22,
            key_type: "ssh-ed25519".to_string(),
            fingerprint: "AAAA...fingerprint".to_string(),
            first_seen_unix: 0,
        },
    );
    revoke_host_key(&mut store, "example.com", 22);
    assert!(store.0.is_empty());
}
