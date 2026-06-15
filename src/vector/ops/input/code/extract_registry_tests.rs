use super::*;

/// Forcing each language's `LazyLock<Registry>` compiles every query in its rule
/// set (a bad S-expression panics at `Query::new` inside `CompiledRule::compile`).
/// Touching the registry and asserting it has rules + valid `decl`/`name` captures
/// proves the whole registry is well-formed.
#[test]
fn all_registry_queries_compile() {
    let registries: [&Registry; 8] = [
        &RUST_REGISTRY,
        &GO_REGISTRY,
        &PYTHON_REGISTRY,
        &JAVASCRIPT_REGISTRY,
        &TYPESCRIPT_REGISTRY,
        &TSX_REGISTRY,
        &BASH_REGISTRY,
        // exercise the dispatch path too
        registry_for(Extractor::Rust).unwrap(),
    ];

    for registry in registries {
        assert!(!registry.rules.is_empty(), "registry has no compiled rules");
        for rule in &registry.rules {
            assert!(
                capture_index(&rule.query, "decl").is_some(),
                "rule missing @decl capture"
            );
            assert!(
                capture_index(&rule.query, "name").is_some(),
                "rule missing @name capture"
            );
        }
    }
}

#[test]
fn tsx_dispatch_routes_to_tsx_registry() {
    // `.tsx` resolves to Extractor::Tsx (via language_for_extension) and must
    // select the JSX grammar; `.ts` (Extractor::TypeScript) selects plain TS.
    let reg = registry_for(Extractor::Tsx).unwrap();
    assert!(std::ptr::eq(reg, &*TSX_REGISTRY));
    let reg = registry_for(Extractor::TypeScript).unwrap();
    assert!(std::ptr::eq(reg, &*TYPESCRIPT_REGISTRY));
}

#[test]
fn tsx_extension_resolves_to_tsx_extractor() {
    // The `.tsx` extension routes to the JSX-aware extractor end-to-end.
    let spec = language_for_extension("tsx").unwrap();
    assert_eq!(spec.extractor, Extractor::Tsx);
    let spec = language_for_extension("ts").unwrap();
    assert_eq!(spec.extractor, Extractor::TypeScript);
}

#[test]
fn none_extractor_has_no_registry() {
    assert!(registry_for(Extractor::None).is_none());
}
