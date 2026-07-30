use super::*;

#[test]
fn docs_family_inventory_matches_the_contract() {
    let slugs = DocsFamily::ALL.map(DocsFamily::slug);
    assert_eq!(
        slugs,
        [
            "cli",
            "cli-help",
            "openapi",
            "mcp",
            "api-dto",
            "api-enums",
            "errors",
            "events",
            "config",
            "env",
            "schema",
            "memory",
            "providers",
            "presentation",
            "schemas",
            "new-source",
        ]
    );
}
