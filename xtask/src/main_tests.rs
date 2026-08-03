use clap::Parser;

use super::Cli;

#[test]
fn generated_contracts_cli_supports_refresh_and_check() {
    for action in ["refresh", "check"] {
        let parsed = Cli::try_parse_from(["xtask", "generated-contracts", action]);
        assert!(parsed.is_ok(), "generated-contracts {action}: {parsed:?}");
    }
}
