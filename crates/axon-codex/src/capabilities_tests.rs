use super::*;

#[test]
fn classifies_requested_control_families() {
    let cases = [
        (
            "account/read",
            CapabilityFamily::Account,
            RiskClass::SensitiveRead,
        ),
        (
            "config/value/write",
            CapabilityFamily::Config,
            RiskClass::Mutation,
        ),
        (
            "mcpServerStatus/list",
            CapabilityFamily::Mcp,
            RiskClass::SafeRead,
        ),
        (
            "skills/config/write",
            CapabilityFamily::Skills,
            RiskClass::Mutation,
        ),
        (
            "plugin/install",
            CapabilityFamily::Plugins,
            RiskClass::Mutation,
        ),
        (
            "marketplace/add",
            CapabilityFamily::Marketplace,
            RiskClass::Mutation,
        ),
        ("app/list", CapabilityFamily::Apps, RiskClass::SafeRead),
        (
            "fs/writeFile",
            CapabilityFamily::Filesystem,
            RiskClass::Execution,
        ),
        (
            "process/spawn",
            CapabilityFamily::Process,
            RiskClass::Execution,
        ),
        (
            "remoteControl/enable",
            CapabilityFamily::RemoteControl,
            RiskClass::Mutation,
        ),
        (
            "thread/start",
            CapabilityFamily::Threads,
            RiskClass::Deferred,
        ),
        (
            "thread/realtime/start",
            CapabilityFamily::Realtime,
            RiskClass::Deferred,
        ),
        (
            "review/start",
            CapabilityFamily::Review,
            RiskClass::Execution,
        ),
        (
            "windowsSandbox/setupStart",
            CapabilityFamily::WindowsSandbox,
            RiskClass::Deferred,
        ),
    ];
    for (method, family, risk) in cases {
        assert_eq!(classify(method, MessageKind::ClientRequest), (family, risk));
    }
}

#[test]
fn server_requests_fail_closed_except_explicit_approvals() {
    assert_eq!(
        classify(
            "item/fileChange/requestApproval",
            MessageKind::ServerRequest
        )
        .1,
        RiskClass::ApprovalRequired
    );
    assert_eq!(
        classify(
            "account/chatgptAuthTokens/refresh",
            MessageKind::ServerRequest
        )
        .1,
        RiskClass::Unsupported
    );
}

#[test]
fn drift_is_sorted_and_actionable() {
    let drift = diff_methods(
        &["account/read", "skills/list"],
        &["plugin/list".into(), "account/read".into()],
    );
    assert_eq!(drift.added, ["plugin/list"]);
    assert_eq!(drift.removed, ["skills/list"]);
}

#[test]
fn unknown_family_is_unsupported() {
    assert_eq!(
        classify("future/mutate", MessageKind::ClientRequest),
        (CapabilityFamily::Other, RiskClass::Unsupported)
    );
}

#[test]
fn checked_inventory_is_sorted_complete_and_classifiable() {
    let inventory: serde_json::Value = serde_json::from_str(METHOD_INVENTORY_JSON).unwrap();
    for (key, kind) in [
        ("client_requests", MessageKind::ClientRequest),
        ("server_requests", MessageKind::ServerRequest),
        ("server_notifications", MessageKind::ServerNotification),
    ] {
        let methods = inventory[key].as_array().unwrap();
        let strings: Vec<_> = methods
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        assert!(strings.windows(2).all(|pair| pair[0] < pair[1]));
        for method in strings {
            let (family, risk) = classify(method, kind);
            assert_ne!(
                family,
                CapabilityFamily::Other,
                "unclassified family: {method}"
            );
            if kind == MessageKind::ClientRequest {
                assert_ne!(
                    risk,
                    RiskClass::Unsupported,
                    "unclassified request: {method}"
                );
            }
        }
    }
}
