use super::*;

#[test]
fn readyz_url_uses_configured_host_and_port() {
    assert_eq!(
        axon_readyz_url("127.0.0.1", 8001),
        "http://127.0.0.1:8001/readyz"
    );
    assert_eq!(
        axon_readyz_url("axon.internal", 9090),
        "http://axon.internal:9090/readyz"
    );
}

#[test]
fn readyz_url_probes_bind_all_over_loopback() {
    for host in ["0.0.0.0", "::", "[::]", "*", "", "  "] {
        assert_eq!(
            axon_readyz_url(host, 8001),
            "http://127.0.0.1:8001/readyz",
            "bind-all host {host:?} should probe loopback"
        );
    }
}

#[test]
fn readyz_url_brackets_ipv6_literal() {
    assert_eq!(axon_readyz_url("::1", 8001), "http://[::1]:8001/readyz");
    // Already-bracketed host is left intact.
    assert_eq!(
        axon_readyz_url("[fe80::1]", 7000),
        "http://[fe80::1]:7000/readyz"
    );
}
