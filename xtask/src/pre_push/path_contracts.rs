pub(super) fn is_generated_contract_path(path: &str) -> bool {
    path.starts_with("docs/pipeline-unification/schemas/")
        || matches!(
            path,
            "docs/pipeline-unification/configuration/config-contract.md"
                | "docs/pipeline-unification/runtime/provider-contract.md"
                | "docs/pipeline-unification/sources/adapter-scopes.md"
                | "docs/pipeline-unification/sources/chunking-contract.md"
                | "docs/pipeline-unification/sources/metadata-payload.md"
                | "docs/pipeline-unification/sources/new-source-contract.md"
        )
        || path.ends_with(".json") && path.starts_with("docs/reference/")
        || path.starts_with("docs/reference/generated/")
        || matches!(
            path,
            "docs/reference/api/dto.md"
                | "docs/reference/api/enums.md"
                | "docs/reference/runtime/observability.md"
                | "docs/reference/runtime/providers.md"
                | "docs/reference/runtime/schema.md"
        )
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
