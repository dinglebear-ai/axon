use axon_api::{SourceKind, SourceRequest, SourceScope};

use crate::{AdapterRegistry, InMemoryAuthorityRegistry, SourceResolver, SourceRouter};

#[test]
fn ci_mcp_seed_fixture_resolves_as_a_local_file() {
    let workflow = include_str!("../../../.github/workflows/ci.yml");
    let seed = workflow
        .split_once("      - name: Seed retrieve fixture\n")
        .expect("MCP smoke seed step exists")
        .1
        .split("\n      - name: ")
        .next()
        .expect("seed step body exists");
    let command = seed
        .lines()
        .find_map(|line| line.trim().strip_prefix("run: "))
        .expect("seed command exists");
    let source = command.split('"').nth(1).expect("fixture source is quoted");
    let mut request = SourceRequest::new(source);
    request.scope = Some(SourceScope::File);
    let registry = AdapterRegistry::target_defaults();
    let resolver = SourceResolver::new(InMemoryAuthorityRegistry::default(), registry.clone());
    let resolved = resolver.resolve(&request).expect("fixture resolves");
    assert_eq!(resolved.source_kind, SourceKind::Local);
    let route = SourceRouter::new(registry)
        .route(&request, resolved)
        .expect("local file route is supported");
    assert_eq!(route.scope, SourceScope::File);
    let arguments = command.split_whitespace().collect::<Vec<_>>();
    assert!(
        arguments.windows(2).any(|pair| pair == ["--scope", "file"]),
        "MCP seed selects file scope"
    );
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(source);
    assert!(fixture.is_file(), "MCP seed fixture exists");
}
