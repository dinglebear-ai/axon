const GENERATED_CONTRACT_PATH_PREFIXES: &[&str] = &[
    "docs/pipeline-unification/schemas/",
    // Every aggregate-generated schema artifact and dependent Markdown
    // projection under docs lives in this canonical output tree. Keep the
    // prefix broad so a new family cannot silently bypass the local gate.
    "docs/reference/",
];

const GENERATED_CONTRACT_INPUT_PATHS: &[&str] = &[
    "docs/pipeline-unification/configuration/config-contract.md",
    "docs/pipeline-unification/runtime/provider-contract.md",
    "docs/pipeline-unification/sources/adapter-scopes.md",
    "docs/pipeline-unification/sources/chunking-contract.md",
    "docs/pipeline-unification/sources/metadata-payload.md",
    "docs/pipeline-unification/sources/new-source-contract.md",
];

pub(super) fn is_generated_contract_path(path: &str) -> bool {
    GENERATED_CONTRACT_PATH_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
        || GENERATED_CONTRACT_INPUT_PATHS.contains(&path)
}

pub(super) fn is_repo_structure_path(path: &str) -> bool {
    path == "Cargo.toml"
        || path == "Cargo.lock"
        || path == "xtask/src/checks/repo_structure.rs"
        || path == "xtask/src/checks/repo_structure_spec.rs"
        || path == "xtask/src/checks/repo_structure_tests.rs"
        || path.starts_with("docs/pipeline-unification/")
        || path.starts_with("crates/")
            && (path.ends_with("/Cargo.toml")
                || path.ends_with("/src/lib.rs")
                || path.ends_with("/src/CLAUDE.md")
                || path.ends_with("/src/AGENTS.md")
                || path.ends_with("/src/GEMINI.md"))
}
