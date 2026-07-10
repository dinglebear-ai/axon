use super::*;
use std::collections::BTreeSet;
use std::path::Path;

/// Extracts the `CliCommand` enum's variant names (kebab-cased, matching
/// clap's default `Subcommand` naming) directly from the live clap source,
/// so this registry can be cross-checked against the real command tree
/// without needing to build/parse `axon --help` output.
fn live_cli_command_names() -> BTreeSet<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a workspace parent");
    let source = std::fs::read_to_string(root.join("crates/axon-core/src/config/cli.rs"))
        .expect("read crates/axon-core/src/config/cli.rs");

    let start = source
        .find("pub(super) enum CliCommand {")
        .expect("CliCommand enum present");
    let body_start = start + source[start..].find('{').unwrap() + 1;
    let rest = &source[body_start..];
    let end = rest
        .find("\n}")
        .expect("CliCommand enum has a closing brace");
    let body = &rest[..end];

    let mut names = BTreeSet::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("///") || trimmed.starts_with("#[") {
            continue;
        }
        // Variant lines look like `Watch(WatchArgs),` (tuple) or `Stats,`
        // (unit) at 4-space indent.
        let ident = if let Some(paren) = trimmed.find('(') {
            &trimmed[..paren]
        } else {
            trimmed.trim_end_matches(',')
        };
        if !ident.is_empty()
            && ident.chars().next().is_some_and(char::is_uppercase)
            && ident.chars().all(|c| c.is_ascii_alphanumeric())
        {
            names.insert(kebab_case(ident));
        }
    }
    names
}

/// All observed `CliCommand` variant identifiers are single CamelCase words
/// (`Watch`, `Endpoints`, `CodeSearch` never appears here), so a lowercase
/// conversion matches clap's kebab-case default. Kept general (word-boundary
/// aware) so this keeps working if a multi-word variant is ever added.
fn kebab_case(ident: &str) -> String {
    let mut out = String::new();
    for (i, ch) in ident.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[test]
fn registry_top_level_groups_match_live_clap_tree_minus_excluded() {
    let live = live_cli_command_names();
    let excluded: BTreeSet<String> = excluded_top_level_groups()
        .iter()
        .map(|s| s.to_string())
        .collect();

    let expected: BTreeSet<String> = live.difference(&excluded).cloned().collect();

    let registered: BTreeSet<String> = command_registry()
        .iter()
        .map(|command| command.path[0].to_string())
        .collect();

    let missing: Vec<_> = expected.difference(&registered).collect();
    assert!(
        missing.is_empty(),
        "live CliCommand groups missing from the xtask CLI registry: {missing:?}"
    );

    let unexpected: Vec<_> = registered.difference(&expected).collect();
    assert!(
        unexpected.is_empty(),
        "xtask CLI registry has groups not present in the live clap tree (or not excluded): {unexpected:?}"
    );
}

#[test]
fn excluded_groups_never_appear_as_registered_top_level_commands() {
    let registered: BTreeSet<&str> = command_registry()
        .iter()
        .map(|command| command.path[0])
        .collect();
    for excluded in excluded_top_level_groups() {
        assert!(
            !registered.contains(excluded),
            "removed-surface command {excluded:?} must not appear in the CLI registry"
        );
    }
}

#[test]
fn every_record_has_a_non_empty_path_and_scope() {
    for command in command_registry() {
        assert!(!command.path.is_empty(), "command record with empty path");
        assert!(
            matches!(command.requires_auth_scope, "read" | "write" | "admin"),
            "unexpected auth scope {:?} for {:?}",
            command.requires_auth_scope,
            command.path
        );
    }
}

#[test]
fn command_records_round_trip_through_json() {
    let records = command_records();
    assert!(!records.is_empty());
    for record in &records {
        assert!(record["name"].is_string());
        assert!(record["requires_auth_scope"].is_string());
        assert!(record.get("maps_to_dto").is_some());
    }
}
