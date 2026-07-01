pub const REQUIRED_WORKSPACE_MEMBERS: &[&str] = &[
    "xtask",
    // axon-error graduated from the PR0 skeleton in Phase 1 (issue #298): it now
    // carries real dependencies and sidecar tests, so it is a required member
    // rather than a `TARGET_CRATES` skeleton entry.
    "crates/axon-error",
    "crates/axon-api",
    "crates/axon-authz",
    "crates/axon-core",
    // Phase 3 / PR4 boundary crates graduated from PR0 skeleton status:
    // they now own store/provider traits, fakes, and sidecar tests.
    "crates/axon-ledger",
    "crates/axon-graph",
    "crates/axon-memory",
    "crates/axon-embedding",
    "crates/axon-vectors",
    "crates/axon-llm",
    "crates/axon-adapters",
    "crates/axon-crawl",
    "crates/axon-vector",
    "crates/axon-ingest",
    "crates/axon-extract",
    "crates/axon-jobs",
    "crates/axon-source-ledger",
    "crates/axon-code-index",
    "crates/axon-services",
    "crates/axon-mcp",
    "crates/axon-web",
    "crates/axon-cli",
];

pub struct TargetCrate {
    pub name: &'static str,
    pub modules: &'static [&'static str],
}

// NOTE: `axon-error` is intentionally NOT listed here. It was filled in during
// Phase 1 (issue #298) — real dependencies, sidecar tests, and public API — so
// it is no longer an empty PR0 skeleton and is instead a required workspace
// member (see `REQUIRED_WORKSPACE_MEMBERS`).
pub const TARGET_CRATES: &[TargetCrate] = &[
    TargetCrate {
        name: "axon-observe",
        modules: &[
            "event",
            "phase",
            "heartbeat",
            "progress",
            "metric",
            "span",
            "log",
            "collector",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-route",
        modules: &[
            "resolver",
            "router",
            "canonical",
            "source_id",
            "scope",
            "authority",
            "alias",
            "capability",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-parse",
        modules: &[
            "parser",
            "registry",
            "facts",
            "graph_candidate",
            "code",
            "manifest",
            "schema",
            "session",
            "tool",
            "env",
            "docker",
            "config",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-document",
        modules: &[
            "preparer",
            "chunk_router",
            "profile",
            "prepared",
            "chunk",
            "metadata",
            "code",
            "markdown",
            "transcript",
            "session",
            "schema",
            "text",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-retrieval",
        modules: &[
            "engine", "plan", "query", "filter", "rank", "context", "citation", "memory", "graph",
            "testing",
        ],
    },
    TargetCrate {
        name: "axon-prune",
        modules: &[
            "plan",
            "executor",
            "debt",
            "generation",
            "orphan",
            "dedupe",
            "receipt",
            "safety",
            "testing",
        ],
    },
];
