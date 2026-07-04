# Phase 7 Parser Metadata Graph Gaps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the Phase 7 parser, metadata, and graph gap work so Docker/env/tool outputs produce contract-valid facts, graph candidates, ranges, redacted payloads, and shared metadata across the source pipeline.

**Architecture:** Keep parsing in `axon-parse`, chunk/source-range validation in `axon-document`, public retrieval payload validation in `axon-vectors`, graph-kind validation in `axon-graph`, and cross-store lineage fixtures in crate-level tests. Parser outputs remain candidates and facts only; durable graph writes stay behind graph-store validation. CLI/MCP tool-output ingestion is metadata-only/no-exec by default and redacts argv/env/stdout/stderr before artifacts or vector writes.

**Tech Stack:** Rust 2024, `serde_json`, `serde_yaml`, `toml`, `axon-api::source` DTOs, `axon-parse`, `axon-document`, `axon-vectors`, `axon-graph`, deterministic fixtures, sibling `*_tests.rs` files.

## Global Constraints

- Source-of-truth docs are `docs/pipeline-unification/sources/metadata-payload.md`, `docs/pipeline-unification/sources/chunking-contract.md`, `docs/pipeline-unification/sources/source-graph.md`, `docs/pipeline-unification/schemas/vector-payload-schema.md`, `docs/pipeline-unification/runtime/redaction-contract.md`, and `docs/pipeline-unification/surfaces/tool-contract.md`.
- Every adapter emits `SourceDocument`; parsers emit `SourceParseFacts` and `GraphCandidate` only.
- Every chunk has deterministic identity, `chunk_locator`, and `source_range`.
- Parser-produced graph facts must not publish invalid source ranges.
- Graph node, edge, and evidence kind strings must come from `source-graph.md`; no alternate names.
- Parser tests may use `axon-graph` as a dev-dependency only. Production `axon-parse` must not depend on `axon-graph`; graph validation stays in `axon-graph`.
- Unknown adapter metadata defaults to `internal` and must not become public by absence of detector hits.
- CLI/MCP tool sources default to metadata-only/no-exec mode.
- Tool execution policy enforcement is a service/action-router concern. Parser output may record untrusted observed claims, but only trusted service audit metadata can create allowed/allowlisted execution facts.
- Tool execution requires explicit opt-in, no shell expansion, command/tool allowlists, environment allowlists, timeout/output caps, and trusted audit metadata before any live execution path.
- Redaction must run through the shared redaction boundary before artifact writes, vector writes, graph evidence, job events, logs/traces, CLI JSON, MCP responses, and REST responses.
- Redaction failure fails closed.
- Parser inputs must be bounded: tool JSONL max line bytes, JSON depth, object/array entries, redacted-field count, and resource refs per record; Compose/YAML max file bytes, alias/depth limits, service count, and candidate fan-out caps.
- Source-range validation must check ordering, normalized document bounds, and quote containment for chunks, parse facts, and graph evidence.
- Do not edit `CLAUDE.md`, `AGENTS.md`, or `GEMINI.md`.
- Commit after each task's verification passes.

---

## Engineering Review Corrections

Apply these corrections before implementation:

- Fix the Docker Compose fixture before writing assertions: do not assert `POSTGRES_PASSWORD` or `qdrant -> postgres` unless the fixture actually contains a `postgres` service and env key.
- Docker/env parser secret detection must call the shared redaction/classification boundary before public writes. Local `is_secret_key` heuristics may only be private hints that are revalidated by the shared boundary.
- Tool-output artifact handling needs explicit artifact-store tests, not only vector payload tests. Cover raw stdout/stderr, content type/disposition, visibility, path traversal, and read-back redaction.
- Compose/YAML parsing must handle aliases and anchors explicitly and enforce candidate fan-out before graph allocation.
- Keep Docker/env parser registration and graph candidate naming in one slice. Tool-output policy and live execution policy should be separate security-sensitive slices.
- First-pass Compose extraction should cover image, service, env, and endpoint facts. Defer volumes, interpolation, and `depends_on` unless fixtures require them.
- Cross-store lineage fixtures must use real builders or shared golden payloads. Avoid large hand-built payload-like objects that can drift from schema validators.
- Avoid dev-dependency cycles between `axon-parse` and `axon-graph`; put cross-store lineage fixtures in an integration/services test crate or choose one dev-dependency direction.

## Source-Of-Truth Alignment Notes

- `chunking-contract.md` requires `code_manifest`, `structured_records`, `api_schema`, `tool_output`, and `session_turns` profiles, plus source ranges for text/code, HTML, transcript, and structured records.
- `source-graph.md` requires Docker/env/tool facts to use canonical node kinds such as `container_image`, `container_image_tag`, `runtime_service`, `network_endpoint`, `environment_variable`, `secret_reference`, `tool`, `tool_call`, `external_resource`, and evidence kinds such as `container_manifest`, `runtime_manifest`, `env_example`, `tool_call_event`, and `tool_result_event`.
- `metadata-payload.md` requires one shared envelope visible in ledger rows, graph evidence, document status, vector payloads, artifacts, memory rows, job status/events, logs/traces, citations, and ask/evaluate traces.
- `vector-payload-schema.md` rejects unknown source-specific public fields unless registered and generated into the payload schema.
- `redaction-contract.md` states unknown metadata is `internal` and redaction failure blocks public payload writes.
- `tool-contract.md` defines CLI/MCP operations as service-routed actions, not arbitrary shell execution.

## Current-State Findings

- `crates/axon-parse/src/docker.rs` and `crates/axon-parse/src/env.rs` exist, but `crates/axon-parse/src/builtins.rs::production_registry` does not register Docker/env parsers.
- `crates/axon-parse/src/docker.rs` uses line heuristics and emits facts, but does not emit contract graph candidates for services/images/endpoints/env keys.
- `crates/axon-parse/src/env.rs` emits env-key facts with `has_default`, but does not classify secret references or graph candidates.
- `crates/axon-parse/src/tool.rs` parses JSONL tool output and artifact refs, but does not enforce metadata-only/no-exec policy, argv/env/stdout/stderr redaction, side-effect class, allowlists, output caps, or external-resource graph nodes.
- `crates/axon-parse/src/graph_candidate.rs` currently emits non-contract graph names (`source_item`, `declares`, `source_line`), which must be replaced before new parser families rely on it.
- `crates/axon-vectors/src/payload_tests.rs` already enforces several required payload and redaction rules, but registry coverage does not yet cover every adapter family or tool-specific field policy.
- `crates/axon-graph/src/candidate.rs` validates closed node/edge kinds and edge evidence, but does not yet enforce evidence-kind names or evidence source-range validity.
- The first Lavra engineering review found these required changes, now incorporated below: graph helpers must model repo/local_checkout/service topology instead of generic `source -> target`; evidence kind validation must use `EvidenceKind`; parser redaction must not fork detector logic from the shared boundary; parser tests must keep `axon-graph` dev-only; tool execution allowlist enforcement must move out of parser; Compose parsing must be bounded structured YAML; metadata registry work must be narrowed to emitted fields plus generated-schema checks; cross-store lineage must use real builders or be deferred.

## File Structure

- Modify: `crates/axon-parse/src/builtins.rs`
  - Register Docker and env parsers in the production registry.
- Modify: `crates/axon-parse/src/docker.rs`
  - Emit bounded Dockerfile and structured Compose facts and graph candidates for images, services, endpoints, volumes, env keys, env files, interpolation, secrets, and `depends_on`.
- Modify: `crates/axon-parse/src/docker_tests.rs`
  - Add production parser and graph candidate fixtures.
- Modify: `crates/axon-parse/src/env.rs`
  - Classify env example keys as environment variables or secret references and redact values.
- Modify: `crates/axon-parse/src/env_tests.rs`
  - Add env example classification and no-secret-value assertions.
- Modify: `crates/axon-parse/src/tool.rs`
  - Parse observed tool-output documents into redacted facts, artifact refs, and resource graph candidates. Treat document-provided execution policy as untrusted observation.
- Modify: `crates/axon-parse/src/tool_tests.rs`
  - Add argv/env/stdout/stderr redaction and execution-policy fixtures.
- Modify: `crates/axon-parse/src/graph_candidate.rs`
  - Replace generic invalid graph candidate helper with typed source-graph contract helpers that accept explicit `from_node_kind/from_stable_key` and `to_node_kind/to_stable_key`.
- Modify: `crates/axon-parse/src/graph_candidate_tests.rs`
  - Add closed-kind graph candidate tests.
- Modify: `crates/axon-document/src/chunk_router.rs`
  - Ensure Docker/env/tool-output inputs route to required chunk profiles.
- Modify: `crates/axon-document/src/chunk_router_tests.rs`
  - Add chunk profile routing tests.
- Modify: `crates/axon-document/src/preparer.rs`
  - Validate source ranges on chunks and graph facts against normalized document bounds before returning prepared documents.
- Modify: `crates/axon-document/src/preparer_tests.rs`
  - Add bad-span degradation fixture.
- Modify: `crates/axon-vectors/src/payload_families.rs`
  - Register only the source-family fields emitted by this plan and prove unknown metadata remains internal.
- Modify: `crates/axon-vectors/src/payload_tests.rs`
  - Add source-family metadata registry tests for every adapter family.
- Modify: `crates/axon-vectors/src/redactor.rs`
  - Ensure unknown adapter metadata remains internal and cannot become public silently.
- Modify: `crates/axon-vectors/src/redactor_tests.rs`
  - Add unknown metadata and tool output redaction tests.
- Modify: `crates/axon-graph/src/candidate.rs`
  - Validate evidence kind names and evidence ranges for parser-produced graph candidates.
- Modify: `crates/axon-graph/src/candidate_tests.rs`
  - Add invalid source-range rejection tests.
- Create: `crates/axon-services/src/source/tool_policy.rs`
  - Own trusted CLI/MCP tool-source execution policy validation outside the parser.
- Test: `crates/axon-services/src/source/tool_policy_tests.rs`
  - Prove no-exec default, opt-in requirements, allowlists, env allowlists, timeout/output caps, and audit metadata.

## Task 1: Register Docker And Env Production Parsers

**Files:**

- Modify: `crates/axon-parse/src/builtins.rs`
- Modify: `crates/axon-parse/src/docker.rs`
- Modify: `crates/axon-parse/src/env.rs`
- Test: `crates/axon-parse/src/parser_tests.rs`

**Interfaces:**

- Consumes: `production_registry() -> ParserRegistry`.
- Produces: production parsers with ids `docker_manifest` and `env_example`.

- [ ] **Step 1: Add failing production registry assertions**

In `crates/axon-parse/src/parser_tests.rs`, extend `production_registry_runs_real_parser_families`:

```rust
let docker = registry.parse(&input(source_doc(
    ContentKind::PlainText,
    Some("Dockerfile"),
    None,
    "FROM qdrant/qdrant:v1.13.1\nEXPOSE 6333\n",
)));
assert_eq!(docker.parser_id, "docker_manifest");
assert!(
    docker
        .facts
        .iter()
        .any(|fact| fact.fact_kind == "docker_base_image" && fact.name == "qdrant/qdrant:v1.13.1")
);

let env = registry.parse(&input(source_doc(
    ContentKind::PlainText,
    Some(".env.example"),
    None,
    "QDRANT_URL=http://localhost:6333\nOPENAI_API_KEY=\n",
)));
assert_eq!(env.parser_id, "env_example");
assert!(
    env.facts
        .iter()
        .any(|fact| fact.fact_kind == "env_var" && fact.name == "QDRANT_URL")
);
assert!(
    env.facts
        .iter()
        .any(|fact| fact.fact_kind == "secret_reference" && fact.name == "OPENAI_API_KEY")
);
```

- [ ] **Step 2: Run the parser registry test and confirm it fails**

Run:

```bash
cargo test -p axon-parse production_registry_runs_real_parser_families --no-fail-fast
```

Expected: FAIL because Docker/env parser families are not registered in `production_registry`.

- [ ] **Step 3: Register parser structs in `builtins.rs`**

Add imports:

```rust
use crate::{code, docker, env, manifest, markdown, schema, session, tool};
```

Add parser structs:

```rust
struct DockerManifestParser;
struct EnvExampleParser;
```

Register them:

```rust
pub fn production_registry() -> ParserRegistry {
    ParserRegistry::new()
        .with_parser(SchemaParser)
        .with_parser(DockerManifestParser)
        .with_parser(EnvExampleParser)
        .with_parser(CodeSymbolsParser)
        .with_parser(ManifestParser)
        .with_parser(MarkdownParser)
        .with_parser(SessionParser)
        .with_parser(ToolParser)
}
```

Implement Docker capability:

```rust
impl SourceParser for DockerManifestParser {
    fn capability(&self) -> &ParserCapability {
        static CAPABILITY: std::sync::OnceLock<ParserCapability> = std::sync::OnceLock::new();
        CAPABILITY.get_or_init(|| ParserCapability {
            parser_id: "docker_manifest".to_string(),
            parser_version: crate::facts::PARSER_VERSION.to_string(),
            content_kinds: vec![ContentKind::PlainText, ContentKind::Yaml],
            mime_types: Vec::new(),
            file_extensions: Vec::new(),
            path_suffixes: vec![
                "Dockerfile".to_string(),
                "Containerfile".to_string(),
                "docker-compose.yml".to_string(),
                "docker-compose.yaml".to_string(),
                "compose.yml".to_string(),
                "compose.yaml".to_string(),
                ".devcontainer/devcontainer.json".to_string(),
            ],
            sniff_prefixes: vec!["FROM ".to_string(), "services:".to_string()],
            priority: 4,
        })
    }

    fn parse(&self, input: &ParseInput) -> ParseResult {
        let (facts, graph_candidates) = docker::docker_parse_items(input);
        completed_result(input, self.capability(), facts, graph_candidates)
    }
}
```

Implement env capability:

```rust
impl SourceParser for EnvExampleParser {
    fn capability(&self) -> &ParserCapability {
        static CAPABILITY: std::sync::OnceLock<ParserCapability> = std::sync::OnceLock::new();
        CAPABILITY.get_or_init(|| ParserCapability {
            parser_id: "env_example".to_string(),
            parser_version: crate::facts::PARSER_VERSION.to_string(),
            content_kinds: vec![ContentKind::PlainText],
            mime_types: Vec::new(),
            file_extensions: Vec::new(),
            path_suffixes: vec![
                ".env.example".to_string(),
                ".env.sample".to_string(),
                ".env.template".to_string(),
                "example.env".to_string(),
                "env.example".to_string(),
                "env.sample".to_string(),
                "env.template".to_string(),
            ],
            sniff_prefixes: Vec::new(),
            priority: 4,
        })
    }

    fn parse(&self, input: &ParseInput) -> ParseResult {
        let (facts, graph_candidates) = env::env_example_parse_items(input);
        completed_result(input, self.capability(), facts, graph_candidates)
    }
}
```

- [ ] **Step 4: Rename parser entry functions**

In `docker.rs`, expose:

```rust
pub fn docker_parse_items(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>)
```

In `env.rs`, expose:

```rust
pub fn env_example_parse_items(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>)
```

Both functions initially return the existing facts and an empty graph candidate list; later tasks fill graph candidates.

- [ ] **Step 5: Run parser tests**

Run:

```bash
cargo test -p axon-parse production_registry_runs_real_parser_families --no-fail-fast
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/axon-parse/src/builtins.rs crates/axon-parse/src/docker.rs crates/axon-parse/src/env.rs crates/axon-parse/src/parser_tests.rs
git commit -m "feat(parse): register docker and env parser families"
```

## Task 2: Replace Invalid Generic Graph Candidate Names And Validate Evidence Kinds

**Files:**

- Modify: `crates/axon-parse/src/graph_candidate.rs`
- Modify: `crates/axon-parse/src/graph_candidate_tests.rs`
- Modify: `crates/axon-graph/src/candidate_tests.rs`
- Modify: `crates/axon-graph/src/candidate.rs`

**Interfaces:**

- Consumes: `ParseInput`, parser id, explicit from/to graph node kinds and stable keys, edge kind, evidence kind, and source range.
- Produces: contract-valid `GraphCandidate` values accepted by `axon_graph::candidate::validate_candidate` without adding a production `axon-graph` dependency to `axon-parse`.

- [ ] **Step 1: Add failing validation test**

In `crates/axon-parse/src/graph_candidate_tests.rs`, add:

```rust
#[test]
fn parser_graph_candidate_uses_closed_source_graph_registry() {
    let input = crate::parser_tests::input(crate::parser_tests::source_doc(
        ContentKind::PlainText,
        Some("Dockerfile"),
        None,
        "FROM postgres:16\n",
    ));

    let candidate = graph_candidate::candidate_edge(
        &input,
        "docker_manifest",
        "container_manifest",
        "local_checkout",
        "local://repo",
        "container_image",
        "docker:library/postgres",
        "repo_uses_container_image",
        "container_manifest",
        Some(1),
        Some("FROM postgres:16".to_string()),
    );

    axon_graph::candidate::validate_candidate(&candidate).expect("candidate is contract-valid");
}
```

If `parser_tests::input` and `source_doc` are private, duplicate those small helpers in `graph_candidate_tests.rs` rather than making test helpers public.

- [ ] **Step 2: Run the new graph-candidate test and confirm it fails**

Run:

```bash
cargo test -p axon-parse parser_graph_candidate_uses_closed_source_graph_registry --no-fail-fast
```

Expected: FAIL because `candidate_edge` does not exist and the current helper emits invalid kind names.

- [ ] **Step 3: Implement the typed contract helper**

Replace or supplement the old helper with a typed helper that does not hardcode a generic `source` node:

```rust
pub fn candidate_edge(
    input: &ParseInput,
    parser_id: &str,
    candidate_kind: &str,
    from_node_kind: &str,
    from_stable_key: &str,
    to_node_kind: &str,
    to_stable_key: &str,
    edge_kind: &str,
    evidence_kind: &str,
    line: Option<u32>,
    quote: Option<String>,
) -> GraphCandidate {
    let evidence_range = line.map(|line| SourceRange {
        line_start: Some(line),
        line_end: Some(line),
        byte_start: None,
        byte_end: None,
        char_start: None,
        char_end: None,
        time_start_ms: None,
        time_end_ms: None,
        dom_selector: None,
        json_pointer: None,
        yaml_path: None,
        xml_xpath: None,
        csv_row: None,
        session_turn_id: None,
        turn_start: None,
        turn_end: None,
    });

    GraphCandidate {
        candidate_id: format!(
            "cand_{}_{}",
            candidate_kind,
            stable_token(&format!("{}:{to_stable_key}:{line:?}", input.document.canonical_uri))
        ),
        job_id: input.job_id,
        source_id: input.document.source_id.clone(),
        source_item_key: input.document.source_item_key.clone(),
        item_canonical_uri: input.document.canonical_uri.clone(),
        document_id: Some(input.document.document_id.clone()),
        kind: candidate_kind.to_string(),
        merge_key: Some(format!("{candidate_kind}:{}:{to_stable_key}", input.document.canonical_uri)),
        producer: GraphCandidateProducer {
            adapter: "axon-parse".to_string(),
            parser: Some(parser_id.to_string()),
            version: PARSER_VERSION.to_string(),
        },
        nodes: vec![
            GraphNodeCandidate {
                node_kind: from_node_kind.to_string(),
                stable_key: from_stable_key.to_string(),
                label: input.document.source_item_key.0.clone(),
                properties: MetadataMap::new(),
            },
            GraphNodeCandidate {
                node_kind: to_node_kind.to_string(),
                stable_key: to_stable_key.to_string(),
                label: to_stable_key.to_string(),
                properties: MetadataMap::new(),
            },
        ],
        edges: vec![GraphEdgeCandidate {
            edge_kind: edge_kind.to_string(),
            from_stable_key: from_stable_key.to_string(),
            to_stable_key: to_stable_key.to_string(),
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id: format!(
                "ev_{}",
                stable_token(&format!("{candidate_kind}:{to_stable_key}:{line:?}:{quote:?}"))
            ),
            evidence_kind: evidence_kind.to_string(),
            source_id: input.document.source_id.clone(),
            source_item_key: input.document.source_item_key.clone(),
            document_id: Some(input.document.document_id.clone()),
            chunk_id: None,
            range: evidence_range,
            quote,
            confidence: 0.9,
            metadata: MetadataMap::new(),
        }],
        confidence: 0.9,
        metadata: MetadataMap::new(),
    }
}
```

- [ ] **Step 4: Validate evidence kind names in graph candidates**

In `crates/axon-graph/src/candidate.rs`, import and apply the closed evidence registry:

```rust
use crate::evidence::EvidenceKind;
```

Inside `validate_candidate`, add:

```rust
for evidence in &candidate.evidence {
    EvidenceKind::from_str(&evidence.evidence_kind)?;
}
```

In `candidate_tests.rs`, add:

```rust
#[test]
fn candidate_validation_rejects_unknown_evidence_kind() {
    let mut candidate = valid_candidate_edge();
    candidate.evidence[0].evidence_kind = "tool_result".to_string();

    let err = validate_candidate(&candidate).expect_err("unknown evidence kind rejected");
    assert!(err.message.contains("unknown graph evidence kind"));
}
```

- [ ] **Step 5: Run parse and graph tests**

Run:

```bash
cargo test -p axon-parse graph_candidate --no-fail-fast
cargo test -p axon-graph candidate --no-fail-fast
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/axon-parse/src/graph_candidate.rs crates/axon-parse/src/graph_candidate_tests.rs crates/axon-graph/src/candidate_tests.rs
git commit -m "fix(parse): emit source graph contract candidates"
```

## Task 3: Emit Docker Service Image Endpoint Env Graph Facts

**Files:**

- Modify: `crates/axon-parse/src/docker.rs`
- Modify: `crates/axon-parse/src/docker_tests.rs`

**Interfaces:**

- Consumes: Dockerfile and Compose `ParseInput`.
- Produces: bounded facts and graph candidates for `container_image`, `container_image_tag`, `runtime_service`, `network_endpoint`, `volume_mount`, `environment_variable`, `secret_reference`, and service dependency edges.

- [ ] **Step 1: Add Dockerfile fixture test**

In `crates/axon-parse/src/docker_tests.rs`, add:

```rust
#[test]
fn dockerfile_parser_emits_image_endpoint_env_and_graph_candidates() {
    let result = parse_fixture(
        "Dockerfile",
        "FROM qdrant/qdrant:v1.13.1\nENV QDRANT__SERVICE__API_KEY=\nEXPOSE 6333\n",
    );

    assert!(has_fact(&result, "docker_base_image", "qdrant/qdrant:v1.13.1"));
    assert!(has_fact(&result, "secret_reference", "QDRANT__SERVICE__API_KEY"));
    assert!(has_fact(&result, "network_endpoint", "6333"));
    assert!(
        result
            .graph_candidates
            .iter()
            .any(|candidate| candidate.nodes.iter().any(|node| node.node_kind == "container_image_tag"))
    );
    assert!(
        result
            .graph_candidates
            .iter()
            .all(|candidate| axon_graph::candidate::validate_candidate(candidate).is_ok())
    );
}
```

- [ ] **Step 2: Add Compose fixture test**

```rust
#[test]
fn compose_parser_emits_service_image_port_volume_and_env_graph_candidates() {
    let result = parse_fixture(
        "docker-compose.yml",
        r#"
services:
  qdrant:
    image: qdrant/qdrant:v1.13.1
    ports:
      - "6333:6333"
    volumes:
      - qdrant-data:/qdrant/storage
    environment:
      QDRANT__SERVICE__API_KEY:
volumes:
  qdrant-data:
"#,
    );

    assert!(has_fact(&result, "runtime_service", "qdrant"));
    assert!(has_fact(&result, "container_image_tag", "qdrant/qdrant:v1.13.1"));
    assert!(has_fact(&result, "network_endpoint", "6333:6333"));
    assert!(has_fact(&result, "volume_mount", "qdrant-data:/qdrant/storage"));
    assert!(has_fact(&result, "secret_reference", "QDRANT__SERVICE__API_KEY"));
    assert!(
        result
            .graph_candidates
            .iter()
            .all(|candidate| axon_graph::candidate::validate_candidate(candidate).is_ok())
    );
}
```

- [ ] **Step 3: Run Docker tests and confirm failures**

Run:

```bash
cargo test -p axon-parse docker --no-fail-fast
```

Expected: FAIL on missing graph candidates and incomplete fact kinds.

- [ ] **Step 4: Implement bounded structured Docker and Compose extraction**

Update `docker_parse_items` to return `(facts, graph_candidates)`. Dockerfiles may keep line parsing with byte and line caps. Compose files must use bounded `serde_yaml` parsing instead of treating every `- ` as a port.

Add parser caps:

```rust
const MAX_DOCKER_FILE_BYTES: usize = 512 * 1024;
const MAX_COMPOSE_DEPTH: usize = 32;
const MAX_COMPOSE_SERVICES: usize = 256;
const MAX_GRAPH_CANDIDATES_PER_DOCUMENT: usize = 2_000;
```

Reject or degrade before parsing when the inline text exceeds `MAX_DOCKER_FILE_BYTES`.

Extract:

```rust
enum DockerFactTarget {
    ImageTag(String),
    Service(String),
    Endpoint(String),
    Volume(String),
    EnvKey(String),
    SecretKey(String),
    ServiceDependency { service: String, dependency: String },
}
```

Classify env keys with:

```rust
fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    ["secret", "password", "token", "api_key", "apikey", "private_key"]
        .iter()
        .any(|needle| lower.contains(needle))
}
```

Emit fact kinds:

```text
docker_base_image
container_image_tag
runtime_service
network_endpoint
volume_mount
environment_variable
secret_reference
service_dependency
```

For each graph-worthy fact, emit candidates using `candidate_edge`:

```rust
candidate_edge(
    input,
    "docker_manifest",
    "container_manifest",
    "local_checkout",
    &local_checkout_key(input),
    "container_image_tag",
    &format!("docker:{image}"),
    "repo_uses_container_image",
    "container_manifest",
    Some(line_no),
    Some(line.to_string()),
)
```

Use `repo_declares_service`, `service_exposes_endpoint`, `service_mounts_volume`, and `service_requires_env` for service-related facts.

For service-scoped edges, the `from_node_kind` must be `runtime_service`, not `source`:

```rust
candidate_edge(
    input,
    "docker_manifest",
    "runtime_manifest",
    "runtime_service",
    &format!("service:{}:{service}", repo_key(input)),
    "network_endpoint",
    &format!("endpoint:{}:{service}:{port}", repo_key(input)),
    "service_exposes_endpoint",
    "runtime_manifest",
    Some(line_no),
    Some(redacted_line),
)
```

Compose structural extraction must include `services.*.image`, `services.*.ports`, `services.*.volumes`, `services.*.environment`, `services.*.env_file`, `services.*.secrets`, `${VAR}` interpolation, and `services.*.depends_on`. Health checks, labels, resources, networks, security options, and inferred URLs are explicitly deferred unless needed by tests.

- [ ] **Step 5: Run Docker tests**

Run:

```bash
cargo test -p axon-parse docker --no-fail-fast
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/axon-parse/src/docker.rs crates/axon-parse/src/docker_tests.rs
git commit -m "feat(parse): extract docker service graph facts"
```

## Task 4: Complete Env Example Extraction Without Secret Values

**Files:**

- Modify: `crates/axon-parse/src/env.rs`
- Modify: `crates/axon-parse/src/env_tests.rs`

**Interfaces:**

- Consumes: `.env.example`, `.env.sample`, `env.example`, and `env.sample`.
- Produces: key-only facts and graph candidates with secret values omitted.

- [ ] **Step 1: Add env secret-value guard test**

In `crates/axon-parse/src/env_tests.rs`, add:

```rust
#[test]
fn env_example_parser_never_emits_secret_values() {
    let result = parse_fixture(
        ".env.example",
        "DATABASE_URL=postgres://user:pass@db/app\nOPENAI_API_KEY=sk-proj-secret\nPORT=3000\n",
    );

    assert!(has_fact(&result, "secret_reference", "DATABASE_URL"));
    assert!(has_fact(&result, "secret_reference", "OPENAI_API_KEY"));
    assert!(has_fact(&result, "environment_variable", "PORT"));
    let serialized = serde_json::to_string(&result).expect("serialize parse result");
    assert!(!serialized.contains("sk-proj-secret"));
    assert!(!serialized.contains("user:pass"));
    assert!(
        result
            .graph_candidates
            .iter()
            .all(|candidate| axon_graph::candidate::validate_candidate(candidate).is_ok())
    );
}
```

- [ ] **Step 2: Run env tests and confirm failure**

Run:

```bash
cargo test -p axon-parse env --no-fail-fast
```

Expected: FAIL until secret classification and graph candidates are added.

- [ ] **Step 3: Implement key-only env facts**

For every parsed env assignment, store only:

```rust
json!({
    "key": key,
    "has_default": !value.trim().is_empty(),
    "value_redacted": !value.trim().is_empty(),
})
```

Emit `secret_reference` when `is_secret_key(key)` or value shape suggests a password URL/token. Emit `environment_variable` otherwise.

- [ ] **Step 4: Emit env graph candidates**

For each key, emit:

```rust
candidate_edge(
    input,
    "env_example",
    "env_example",
    "local_checkout",
    &local_checkout_key(input),
    if secret { "secret_reference" } else { "environment_variable" },
    &format!("{}:{}", if secret { "secret" } else { "env" }, key),
    "repo_declares_env_var",
    "env_example",
    Some(line_no),
    Some(format!("{key}=<redacted>")),
)
```

- [ ] **Step 5: Run env tests**

Run:

```bash
cargo test -p axon-parse env --no-fail-fast
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/axon-parse/src/env.rs crates/axon-parse/src/env_tests.rs
git commit -m "feat(parse): extract env example graph facts safely"
```

## Task 5: Parse Tool Output Safely And Keep Execution Policy In Services

**Files:**

- Modify: `crates/axon-parse/src/tool.rs`
- Modify: `crates/axon-parse/src/tool_tests.rs`
- Create: `crates/axon-services/src/source/tool_policy.rs`
- Test: `crates/axon-services/src/source/tool_policy_tests.rs`

**Interfaces:**

- Consumes: JSONL tool-output source documents plus trusted service audit metadata for live execution.
- Produces: bounded redacted observed facts, artifact refs, external-resource graph candidates, and service-owned execution policy validation.

- [ ] **Step 1: Add no-exec and redaction fixture**

In `crates/axon-parse/src/tool_tests.rs`, add:

```rust
#[test]
fn tool_output_parser_defaults_to_metadata_only_and_redacts_io() {
    let result = parse_fixture(
        "tool-output.jsonl",
        r#"{"tool":"shell","action":"exec","execution_requested":true,"execution_allowed":false,"side_effect_class":"read","argv":["curl","-H","Authorization: Bearer abc","https://api.example.com"],"env":{"OPENAI_API_KEY":"sk-proj-secret","PATH":"/usr/bin"},"stdout":"token=ghp_secret","stderr":"password=secret","output":{"artifact_id":"art_1","size_bytes":70000,"reason":"oversized stdout"},"resources":[{"kind":"github_issue","uri":"https://github.com/jmagar/axon/issues/298"}]}"#,
    );

    assert!(has_fact(&result, "tool_observed_claim", "shell.exec"));
    assert!(has_fact(&result, "tool_artifact_ref", "art_1"));
    assert!(has_fact(&result, "external_resource", "https://github.com/jmagar/axon/issues/298"));
    let serialized = serde_json::to_string(&result).expect("serialize parse result");
    assert!(!serialized.contains("sk-proj-secret"));
    assert!(!serialized.contains("Bearer abc"));
    assert!(!serialized.contains("ghp_secret"));
    assert!(!serialized.contains("password=secret"));
    assert!(
        result
            .graph_candidates
            .iter()
            .all(|candidate| axon_graph::candidate::validate_candidate(candidate).is_ok())
    );
}
```

- [ ] **Step 2: Add bounded JSONL parsing fixture**

```rust
#[test]
fn tool_output_parser_degrades_oversized_jsonl_before_parsing() {
    let huge = format!(
        "{{\"tool\":\"mcp\",\"output\":\"{}\"}}",
        "x".repeat(MAX_TOOL_JSONL_LINE_BYTES + 1)
    );
    let result = parse_fixture("tool-output.jsonl", &huge);

    assert_eq!(result.header.status, LifecycleStatus::CompletedDegraded);
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.code == "tool.jsonl.line_too_large")
    );
    assert!(result.facts.is_empty());
    assert!(result.graph_candidates.is_empty());
}
```

- [ ] **Step 3: Add trusted service policy validation fixtures**

Create `crates/axon-services/src/source/tool_policy_tests.rs` with:

```rust
#[test]
fn tool_source_execution_requires_trusted_policy_snapshot() {
    let request = ToolSourceExecutionRequest {
        source_kind: SourceKind::CliTool,
        execution_requested: true,
        command: vec!["sh".to_string(), "-c".to_string(), "echo hi".to_string()],
        env: BTreeMap::from([("OPENAI_API_KEY".to_string(), "sk-proj-secret".to_string())]),
        timeout_ms: None,
        output_cap_bytes: None,
        audit_snapshot: None,
    };

    let err = validate_tool_source_execution(&request)
        .expect_err("trusted audit snapshot is required");
    assert_eq!(err.code, "tool.execution_policy_missing");
}

#[test]
fn tool_source_execution_accepts_explicit_no_shell_allowlisted_policy() {
    let request = ToolSourceExecutionRequest {
        source_kind: SourceKind::McpTool,
        execution_requested: true,
        command: vec!["mcp-call".to_string(), "server.tool".to_string()],
        env: BTreeMap::from([("PATH".to_string(), "/usr/bin".to_string())]),
        timeout_ms: Some(30_000),
        output_cap_bytes: Some(64 * 1024),
        audit_snapshot: Some(ToolExecutionAuditSnapshot {
            policy_id: "policy_tool_read".to_string(),
            side_effect_class: "read".to_string(),
            command_allowlist: vec!["mcp-call".to_string()],
            env_allowlist: vec!["PATH".to_string()],
            shell_expansion_allowed: false,
        }),
    };

    validate_tool_source_execution(&request).expect("trusted policy accepted");
}
```

- [ ] **Step 4: Run tool parser and service policy tests and confirm failure**

Run:

```bash
cargo test -p axon-parse tool --no-fail-fast
cargo test -p axon-services tool_policy --no-fail-fast
```

Expected: FAIL until bounded parsing and service policy validation are implemented.

- [ ] **Step 5: Implement bounded observed tool-output parsing**

Add parser caps:

```rust
pub const MAX_TOOL_JSONL_LINE_BYTES: usize = 256 * 1024;
pub const MAX_TOOL_JSON_DEPTH: usize = 32;
pub const MAX_TOOL_JSON_ENTRIES: usize = 4_096;
pub const MAX_TOOL_REDACTED_FIELDS: usize = 512;
pub const MAX_TOOL_RESOURCES_PER_RECORD: usize = 128;
```

Before `serde_json::from_str`, reject oversized lines. After parsing, walk the value with an explicit stack that enforces depth and entry caps. Degrade the parse result when caps are exceeded.

Parser facts must treat document-provided execution flags as untrusted observed claims:

```rust
json!({
    "tool": tool,
    "action": action,
    "observed_execution_requested": value.get("execution_requested").and_then(Value::as_bool),
    "observed_execution_allowed_claim": value.get("execution_allowed").and_then(Value::as_bool),
    "trusted_policy": false,
    "argv": "[redacted]",
    "env": "[redacted]",
    "stdout": "[redacted]",
    "stderr": "[redacted]"
})
```

- [ ] **Step 6: Implement service-owned no-exec policy**

Create `crates/axon-services/src/source/tool_policy.rs`:

```rust
pub struct ToolExecutionAuditSnapshot {
    pub policy_id: String,
    pub side_effect_class: String,
    pub command_allowlist: Vec<String>,
    pub env_allowlist: Vec<String>,
    pub shell_expansion_allowed: bool,
}

pub struct ToolSourceExecutionRequest {
    pub source_kind: SourceKind,
    pub execution_requested: bool,
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub timeout_ms: Option<u64>,
    pub output_cap_bytes: Option<u64>,
    pub audit_snapshot: Option<ToolExecutionAuditSnapshot>,
}

pub fn validate_tool_source_execution(request: &ToolSourceExecutionRequest) -> Result<(), ApiError>
```

Rules:

- `execution_requested=false` passes as metadata-only/no-exec.
- `execution_requested=true` requires `audit_snapshot`.
- Command vectors are matched by first argv element; no shell expansion is accepted unless the trusted snapshot explicitly permits it.
- Every env key must be in `env_allowlist`.
- `timeout_ms` and `output_cap_bytes` are required and must be nonzero.
- Document-provided JSONL claims never populate `audit_snapshot`.

- [ ] **Step 7: Emit tool graph candidates**

Emit:

- `tool_call -> tool` via `tool_call_uses_tool`
- `tool_call -> external_resource` via `tool_call_read_resource` or `tool_call_mutated_resource`
- artifact refs via `tool_call_produced_artifact`

Use evidence kinds `tool_call_event` and `tool_result_event`. Limit emitted resource graph candidates to `MAX_TOOL_RESOURCES_PER_RECORD`.

- [ ] **Step 8: Run tool tests**

Run:

```bash
cargo test -p axon-parse tool --no-fail-fast
cargo test -p axon-services tool_policy --no-fail-fast
```

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/axon-parse/src/tool.rs crates/axon-parse/src/tool_tests.rs crates/axon-services/src/source/tool_policy.rs crates/axon-services/src/source/tool_policy_tests.rs
git commit -m "feat(parse): safely ingest observed tool output"
```

## Task 6: Validate Source Ranges Across Chunk Profiles, Parse Facts, And Graph Evidence

**Files:**

- Modify: `crates/axon-document/src/chunk_router.rs`
- Modify: `crates/axon-document/src/chunk_router_tests.rs`
- Modify: `crates/axon-document/src/preparer.rs`
- Modify: `crates/axon-document/src/preparer_tests.rs`
- Modify: `crates/axon-graph/src/candidate.rs`
- Modify: `crates/axon-graph/src/candidate_tests.rs`

**Interfaces:**

- Consumes: normalized source document text and `SourceRange` on chunks, parse facts, and graph evidence.
- Produces: degraded parse/preparation results for invalid spans before publish, including ordering, document bounds, and quote containment checks.

- [ ] **Step 1: Add chunk-router profile tests**

In `crates/axon-document/src/chunk_router_tests.rs`, assert routing:

```rust
assert_eq!(route_for_path("Dockerfile").chunking_profile, "code_manifest");
assert_eq!(route_for_path("docker-compose.yml").chunking_profile, "code_manifest");
assert_eq!(route_for_path(".env.example").chunking_profile, "structured_records");
assert_eq!(route_for_path("tool-output.jsonl").chunking_profile, "tool_output");
```

- [ ] **Step 2: Add graph candidate bad-span rejection test**

In `crates/axon-graph/src/candidate_tests.rs`, add:

```rust
#[test]
fn candidate_validation_rejects_invalid_evidence_source_range() {
    let mut candidate = valid_candidate_edge();
    candidate.evidence[0].range = Some(SourceRange {
        line_start: Some(10),
        line_end: Some(3),
        byte_start: None,
        byte_end: None,
        char_start: None,
        char_end: None,
        time_start_ms: None,
        time_end_ms: None,
        dom_selector: None,
        json_pointer: None,
        yaml_path: None,
        xml_xpath: None,
        csv_row: None,
        session_turn_id: None,
        turn_start: None,
        turn_end: None,
    });

    let err = validate_candidate(&candidate).expect_err("invalid source range rejected");
    assert!(err.message.contains("invalid source range"));
}
```

- [ ] **Step 3: Add normalized document bounds test**

In `crates/axon-document/src/preparer_tests.rs`, add:

```rust
#[test]
fn preparer_degrades_chunk_and_parse_fact_ranges_outside_normalized_document() {
    let doc = source_document_with_text(".env.example", "PORT=3000\n");
    let mut prepared = prepare_document_for_test(doc);
    prepared.chunks[0].source_range.line_start = Some(9000);
    prepared.chunks[0].source_range.line_end = Some(9001);

    let err = validate_prepared_document_ranges(&prepared)
        .expect_err("range outside normalized document rejected");
    assert!(err.to_string().contains("outside normalized document"));
}
```

- [ ] **Step 4: Run range tests and confirm failure**

Run:

```bash
cargo test -p axon-document chunk_router --no-fail-fast
cargo test -p axon-graph candidate_validation_rejects_invalid_evidence_source_range --no-fail-fast
```

Expected: FAIL until routing/range validation is implemented.

- [ ] **Step 5: Implement source range validator**

Create a shared range validator in `crates/axon-document/src/source_range.rs`:

```rust
pub struct SourceRangeBounds {
    pub line_count: u32,
    pub byte_len: u64,
    pub char_count: u64,
}

pub fn bounds_for_text(text: &str) -> SourceRangeBounds {
    SourceRangeBounds {
        line_count: text.lines().count().max(1) as u32,
        byte_len: text.len() as u64,
        char_count: text.chars().count() as u64,
    }
}

pub fn validate_source_range(range: &SourceRange, bounds: &SourceRangeBounds) -> Result<(), ApiError> {
    if let (Some(start), Some(end)) = (range.line_start, range.line_end)
        && (start > end || end > bounds.line_count)
    {
        return Err(validation_error("invalid source range outside normalized document"));
    }
    if let (Some(start), Some(end)) = (range.byte_start, range.byte_end)
        && (start > end || end > bounds.byte_len)
    {
        return Err(validation_error("invalid source range outside normalized document"));
    }
    if let (Some(start), Some(end)) = (range.char_start, range.char_end)
        && start > end
    {
        return Err(validation_error("invalid source range: char_start > char_end"));
    }
    Ok(())
}
```

Use this validator from document preparation for chunks and parse facts. In `axon-graph`, keep ordering validation for graph-only calls and add a graph-store validation overload that accepts normalized document bounds before publication. Graph evidence with a quote must also prove the quote appears inside the normalized source text covered by the range.

- [ ] **Step 6: Implement chunk-router profile mapping**

Add path checks in `chunk_router.rs` before plain text fallback:

```rust
if path.ends_with("Dockerfile") || path.ends_with("Containerfile") || path.ends_with("docker-compose.yml") || path.ends_with("docker-compose.yaml") {
    return ChunkRoute::profile("code_manifest", "structured_parser");
}
if path.ends_with(".env.example") || path.ends_with(".env.sample") || path.ends_with(".env.template") || path.ends_with("example.env") {
    return ChunkRoute::profile("structured_records", "env_parser");
}
if path.ends_with("tool-output.jsonl") {
    return ChunkRoute::profile("tool_output", "tool_output_jsonl");
}
```

Use the existing `ChunkRoute` constructor shape in the file.

- [ ] **Step 7: Run document and graph tests**

Run:

```bash
cargo test -p axon-document --no-fail-fast
cargo test -p axon-graph --no-fail-fast
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/axon-document/src/chunk_router.rs crates/axon-document/src/chunk_router_tests.rs crates/axon-document/src/preparer.rs crates/axon-document/src/preparer_tests.rs crates/axon-graph/src/candidate.rs crates/axon-graph/src/candidate_tests.rs
git commit -m "fix(graph): reject invalid parser source ranges"
```

## Task 7: Complete Metadata Registry And Unknown Metadata Policy

**Files:**

- Modify: `crates/axon-vectors/src/payload_families.rs`
- Modify: `crates/axon-vectors/src/payload_tests.rs`
- Modify: `crates/axon-vectors/src/redactor.rs`
- Modify: `crates/axon-vectors/src/redactor_tests.rs`

**Interfaces:**

- Consumes: source-family metadata field registry and generated vector payload schema.
- Produces: tests for Phase 7-emitted metadata fields, internal-default unknown metadata, redaction failure blocking, and generated schema drift.

- [ ] **Step 1: Add source-family registry test for Phase 7 emitted fields**

In `crates/axon-vectors/src/payload_tests.rs`, add:

```rust
#[test]
fn source_family_registry_covers_phase_7_emitted_fields() {
    for (family, field) in [
        ("code", "code_language"),
        ("graph", "graph_node_ids"),
        ("local", "local_checkout"),
        ("tool", "tool_name"),
        ("tool", "tool_action"),
        ("tool", "tool_side_effect_class"),
        ("tool", "tool_output_artifact_id"),
        ("docker", "docker_image"),
        ("docker", "docker_service"),
        ("docker", "docker_port"),
        ("docker", "docker_volume"),
        ("env", "env_key"),
        ("env", "env_secret_reference"),
    ] {
        assert!(
            crate::payload_families::source_family_allows_field(family, field),
            "missing metadata registry for {family}.{field}"
        );
    }
}
```

- [ ] **Step 2: Add unknown metadata internal-default test**

In `crates/axon-vectors/src/redactor_tests.rs`, add:

```rust
#[test]
fn unknown_adapter_metadata_defaults_to_internal() {
    let redactor = VectorPayloadRedactor::default();
    let metadata = serde_json::json!({
        "adapter_blob": { "raw": "not classified" },
        "source_family": "web"
    });

    let report = redactor.classify_metadata(&metadata);

    assert_eq!(report.field_visibility("adapter_blob"), Some(Visibility::Internal));
    assert!(!report.public_fields().contains(&"adapter_blob".to_string()));
}
```

- [ ] **Step 3: Run vector payload tests and confirm failure**

Run:

```bash
cargo test -p axon-vectors payload --no-fail-fast
```

Expected: FAIL until registry and redactor behavior are complete.

- [ ] **Step 4: Add redaction failure blocks public write test**

In `crates/axon-vectors/src/redactor_tests.rs`, add:

```rust
#[test]
fn redaction_failure_blocks_public_payload_write() {
    let redactor = VectorPayloadRedactor::failing_for_test("detector crashed");
    let mut metadata = valid_payload_fixture("web.valid.json");
    metadata.insert("chunk_text".to_string(), serde_json::json!("Authorization: Bearer abc"));

    let err = redactor
        .redact_public_payload(metadata)
        .expect_err("redaction failure must fail closed");

    assert_eq!(err.code(), "redaction.failed");
}
```

- [ ] **Step 5: Add registry records**

Add family records in `payload_families.rs` only for fields emitted by this plan:

```text
local_: local_checkout, local_path_key, local_git_remote, local_git_commit
tool_: tool_name, tool_action, tool_side_effect_class, tool_output_artifact_id
docker_: docker_image, docker_service, docker_port, docker_volume
env_: env_key, env_secret_reference
```

Each registered field must include owner, visibility, and redaction classification in the same structure used by existing families.

- [ ] **Step 6: Implement internal-default unknown metadata**

Update the redactor classification so an unknown key returns `Visibility::Internal`, and public payload construction rejects it unless it is registry-approved.

- [ ] **Step 7: Run vector and schema drift checks**

Run:

```bash
cargo test -p axon-vectors payload --no-fail-fast
cargo test -p axon-vectors redactor --no-fail-fast
cargo xtask schemas vector-payload --check
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/axon-vectors/src/payload_families.rs crates/axon-vectors/src/payload_tests.rs crates/axon-vectors/src/redactor.rs crates/axon-vectors/src/redactor_tests.rs
git commit -m "fix(vectors): complete source metadata registry"
```

## Task 8: Add Builder-Backed Ledger Vector Graph Lineage Fixtures

**Files:**

- Create: `crates/axon-graph/src/fixture_tests.rs`
- Modify: `crates/axon-graph/src/lib.rs`
- Modify: `crates/axon-vectors/src/payload_tests.rs`

**Interfaces:**

- Consumes: source generation metadata, the real vector payload validator/builder path, and typed graph candidate helpers from `axon-parse`.
- Produces: fixtures proving `source_id`, `source_generation`, `committed_generation`, `document_id`, `chunk_id`, `job_id`, and evidence ranges line up across stores without bypassing production validation.

- [ ] **Step 1: Add graph fixture test module**

In `crates/axon-graph/src/lib.rs`, add:

```rust
#[cfg(test)]
#[path = "fixture_tests.rs"]
mod fixture_tests;
```

- [ ] **Step 2: Create cross-store lineage test using real validators**

Create `crates/axon-graph/src/fixture_tests.rs`:

```rust
use axon_api::source::*;
use uuid::Uuid;

use crate::candidate::validate_candidate;

#[test]
fn ledger_vector_graph_fixture_shares_source_generation_lineage() {
    let source_id = SourceId::from("src_local_repo");
    let generation = 7_i64;
    let document_id = DocumentId::from("doc_Dockerfile");
    let chunk_id = ChunkId::from("chunk_Dockerfile_1");
    let job_id = JobId::new(Uuid::from_u128(7));

    let vector_payload = VectorPayload::try_from_metadata(MetadataMap::from_json(serde_json::json!({
        "payload_contract_version": "2026-06-30",
        "collection": "axon",
        "vector_point_id": "point_1",
        "vector_namespace": "source",
        "source_family": "local",
        "source_id": source_id.0,
        "source_kind": "local",
        "source_adapter": "local",
        "source_scope": "repo",
        "source_canonical_uri": "file:///repo",
        "source_generation": generation,
        "committed_generation": generation,
        "source_item_key": "Dockerfile",
        "item_canonical_uri": "file:///repo/Dockerfile",
        "document_id": document_id.0,
        "chunk_id": chunk_id.0,
        "chunk_index": 0,
        "content_kind": "structured",
        "content_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "chunk_hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "chunk_text": "Dockerfile image qdrant/qdrant:v1.13.1 exposes 6333",
        "chunk_locator": {
            "canonical_uri": "file:///repo/Dockerfile",
            "path": "Dockerfile",
            "range": { "line_start": 1, "line_end": 2 }
        },
        "source_range": { "line_start": 1, "line_end": 2 },
        "redaction_status": "clean",
        "visibility": "public",
        "job_id": job_id.to_string(),
        "document_status": "published",
        "embedding_model": "fake",
        "embedding_dimensions": 8,
        "embedding_provider": "fake",
        "embedding_profile": "test",
        "embedded_at": "2026-07-04T00:00:00Z"
    })))
    .expect("vector payload validates through production validator");

    assert_eq!(vector_payload.metadata()["source_id"], source_id.0);
    assert_eq!(vector_payload.metadata()["source_generation"], generation);
    assert_eq!(vector_payload.metadata()["committed_generation"], generation);
    assert_eq!(vector_payload.metadata()["document_id"], document_id.0);
    assert_eq!(vector_payload.metadata()["chunk_id"], chunk_id.0);

    let candidate = axon_parse::graph_candidate::candidate_edge(
        &parse_input_for_document("Dockerfile", "FROM qdrant/qdrant:v1.13.1\n"),
        "docker_manifest",
        "container_manifest",
        "local_checkout",
        "local://src_local_repo",
        "container_image_tag",
        "docker:qdrant/qdrant:v1.13.1",
        "repo_uses_container_image",
        "container_manifest",
        Some(1),
        Some("FROM qdrant/qdrant:v1.13.1".to_string()),
    );
    /*
    The candidate helper above replaces the old hand-built fixture body:
    let candidate = GraphCandidate {
        candidate_id: "cand_container_image".to_string(),
        job_id,
        source_id: source_id.clone(),
        source_item_key: SourceItemKey::from("Dockerfile"),
        item_canonical_uri: "file:///repo/Dockerfile".to_string(),
        document_id: Some(document_id),
        kind: "container_manifest".to_string(),
        merge_key: Some("container_manifest:file:///repo/Dockerfile:qdrant".to_string()),
        producer: GraphCandidateProducer {
            adapter: "axon-parse".to_string(),
            parser: Some("docker_manifest".to_string()),
            version: "test".to_string(),
        },
        nodes: vec![
            GraphNodeCandidate {
                node_kind: "source".to_string(),
                stable_key: "source:src_local_repo".to_string(),
                label: "src_local_repo".to_string(),
                properties: MetadataMap::new(),
            },
            GraphNodeCandidate {
                node_kind: "container_image_tag".to_string(),
                stable_key: "docker:qdrant/qdrant:v1.13.1".to_string(),
                label: "qdrant/qdrant:v1.13.1".to_string(),
                properties: MetadataMap::new(),
            },
        ],
        edges: vec![GraphEdgeCandidate {
            edge_kind: "repo_uses_container_image".to_string(),
            from_stable_key: "source:src_local_repo".to_string(),
            to_stable_key: "docker:qdrant/qdrant:v1.13.1".to_string(),
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id: "ev_1".to_string(),
            evidence_kind: "container_manifest".to_string(),
            source_id,
            source_item_key: SourceItemKey::from("Dockerfile"),
            document_id: Some(DocumentId::from("doc_Dockerfile")),
            chunk_id: Some(chunk_id),
            range: Some(SourceRange {
                line_start: Some(1),
                line_end: Some(1),
                byte_start: None,
                byte_end: None,
                char_start: None,
                char_end: None,
                time_start_ms: None,
                time_end_ms: None,
                dom_selector: None,
                json_pointer: None,
                yaml_path: None,
                xml_xpath: None,
                csv_row: None,
                session_turn_id: None,
                turn_start: None,
                turn_end: None,
            }),
            quote: Some("FROM qdrant/qdrant:v1.13.1".to_string()),
            confidence: 0.9,
            metadata: MetadataMap::new(),
        }],
        confidence: 0.9,
        metadata: MetadataMap::new(),
    };
    */

    validate_candidate(&candidate).expect("fixture candidate validates");
}
```

- [ ] **Step 3: Run graph fixture test**

Run:

```bash
cargo test -p axon-graph ledger_vector_graph_fixture_shares_source_generation_lineage --no-fail-fast
```

Expected: PASS after previous tasks.

- [ ] **Step 4: Commit**

```bash
git add crates/axon-graph/src/fixture_tests.rs crates/axon-graph/src/lib.rs crates/axon-vectors/src/payload_tests.rs
git commit -m "test(graph): tie source generation lineage fixtures"
```

## Task 9: Final Verification And Checklist Evidence

**Files:**

- Modify if needed: `docs/pipeline-unification/plans/2026-07-04-phase-7-parser-metadata-graph-gaps.md`
- Modify after implementation: GitHub issue #298 checklist.

**Interfaces:**

- Consumes: all prior task commits.
- Produces: evidence for Phase 7 Task 2 checklist completion.

- [ ] **Step 1: Run requested checks**

Run:

```bash
cargo test -p axon-parse --no-fail-fast
cargo test -p axon-document --no-fail-fast
cargo test -p axon-vectors payload --no-fail-fast
cargo test -p axon-graph --no-fail-fast
cargo test -p axon-services tool_policy --no-fail-fast
cargo xtask schemas vector-payload --check
```

Expected: PASS.

- [ ] **Step 2: Run contract drift searches**

Run:

```bash
rg -n '"source_item"|"declares"|"source_line"' crates/axon-parse crates/axon-graph
rg -n 'sk-proj|Authorization: Bearer|ghp_|password=' crates/axon-parse/src/*_tests.rs crates/axon-vectors/tests/fixtures
rg -n 'candidate_''with_edge|tool_execution_''policy' crates/axon-parse crates/axon-services
```

Expected:
- No production graph candidate helper emits non-contract `source_item`, `declares`, or `source_line` names.
- Secret-like fixture values appear only in negative tests that assert they are rejected or redacted.
- No parser production code emits trusted execution-policy facts from untrusted JSONL claims.

- [ ] **Step 3: Update issue #298 checklist after implementation**

Use:

```bash
gh issue view 298 --json body > /tmp/issue-298.json
```

Check off only the Phase 7 Task 2 items proven by the passing checks and add an issue comment with the exact commands from Step 1.

- [ ] **Step 4: Commit plan/evidence updates**

```bash
git add docs/pipeline-unification/plans/2026-07-04-phase-7-parser-metadata-graph-gaps.md
git commit -m "docs(pipeline): plan phase 7 parser metadata graph gaps"
```

## Self-Review

- Spec coverage: Docker/env parser production registration is covered by Tasks 1, 3, and 4. Source-range validation is covered by Task 6. Service/env/endpoint/toolchain fact extraction is covered by Tasks 3, 4, and 5, with bounded Compose scope in Task 3. CLI/MCP tool-output policy and no-exec defaults are covered by service-owned policy validation in Task 5. Metadata registry coverage, unknown metadata policy, redaction fail-closed behavior, and generated schema checks are covered by Task 7. Shared metadata lineage across vector/graph/generation is covered by Task 8 through production validators. Bad spans and redacted tool output failure guards are covered by Tasks 5 and 6.
- Placeholder scan: the plan contains no deferred placeholder language.
- Type consistency: the plan uses existing `ParseInput`, `ParseResult`, `ParserCapability`, `SourceParser`, `SourceParseFacts`, `GraphCandidate`, `GraphEvidence`, `SourceRange`, `MetadataMap`, and `production_registry` concepts from the current crates.
