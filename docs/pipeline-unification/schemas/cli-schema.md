# CLI Schema Contract
Last Modified: 2026-07-14

## Contract

The CLI schema is generated from the `axon-cli` clap command model. It provides
machine-readable command names, flags, defaults, enums, help text, and
command-to-DTO mappings.

Human behavior lives in [../surfaces/command-contract.md](../surfaces/command-contract.md)
and target help text lives in [../surfaces/axon-help.md](../surfaces/axon-help.md).

## Generated Artifacts

This section is the target generated-artifact contract. The current `xtask`
surface does not yet expose `cargo xtask schemas cli`, and `docs/reference/cli`
does not exist yet. Until the generator lands, CLI drift is checked by the
current parser/help contract tests and by direct review of
`crates/axon-core/src/config`.

```text
docs/reference/cli/commands.json
docs/reference/cli/commands.md
docs/reference/cli/axon-help.md
```

Generator:

```bash
cargo xtask schemas cli
cargo xtask schemas cli --check
```

## Required Schema Fields

Every command record includes:

- `name`
- `aliases` empty unless explicitly allowed by contract
- `summary`
- `usage`
- `args`
- `flags`
- `env_overrides`
- `maps_to_dto`
- `mutates`
- `async`
- `requires_auth_scope`
- `examples`

## Root Artifact Shape

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://axon.local/schemas/cli/commands.schema.json",
  "title": "AxonCliCommands",
  "x-axon": {
    "owner_crates": ["axon-cli", "axon-api"],
    "generated_by": "cargo xtask schemas cli",
    "contract_version": "2026-06-30",
    "source_inputs": [
      "crates/axon-cli/src",
      "crates/axon-api/src",
      "crates/axon-core/src/config"
    ]
  },
  "commands": []
}
```

## Command Record Shape

```json
{
  "name": "source",
  "path": ["axon", "source"],
  "aliases": [],
  "summary": "Acquire, normalize, embed, refresh, and optionally watch a source.",
  "usage": "axon <source> [options]",
  "maps_to_dto": "SourceRequest",
  "service": "SourceService.submit",
  "mutates": true,
  "async": true,
  "requires_auth_scope": "axon:write",
  "args": [],
  "flags": [],
  "examples": []
}
```

Aliases must be empty unless a contract explicitly allows one. For this clean
break, removed commands do not appear as aliases.

Every flag record includes:

- long name
- short name when present
- value type
- enum values when applicable
- default
- repeatability
- required status
- config/env mapping when applicable

## Flag Record Shape

```json
{
  "long": "watch",
  "short": null,
  "value_type": "boolean",
  "required": false,
  "repeatable": false,
  "default": false,
  "enum_values": null,
  "maps_to_field": "SourceRequest.watch",
  "env_override": null,
  "config_key": null,
  "help": "Create or ensure a durable watch for this source."
}
```

## Required Command Families

- source/default source invocation
- watch lifecycle
- map
- extract
- search/query/retrieve/ask/chat/evaluate/suggest/research/summarize
- memory
- graph
- jobs
- providers/status/doctor/capabilities
- preflight/smoke
- prune/collections/artifacts/uploads
- config

Removed commands are absent.

`scrape` is the only retained former source-family convenience command. Its
schema must map to `SourceRequest` with `scope=page`, `embed=true` by default,
`limits.max_pages=1`, and clean content output. It must not expose crawl fanout
or a legacy scrape engine path.

Reserved removed command tokens (`crawl`, `embed`, `ingest`, `code-search`,
and `code-search-watch`) do not appear as commands or aliases. A CLI schema
generator may record them only as removed/reserved diagnostics with replacement
guidance; they must not deserialize to executable command specs.

## Complete Command Registry Shape

`axon-cli` owns a command registry derived from clap plus explicit DTO mapping:

```rust
pub struct CliCommandSpec {
    pub path: &'static [&'static str],
    pub summary: &'static str,
    pub maps_to_dto: &'static str,
    pub service: &'static str,
    pub mutates: bool,
    pub async_job: bool,
    pub required_scope: Option<AuthScope>,
    pub args: &'static [CliArgSpec],
    pub flags: &'static [CliFlagSpec],
    pub examples: &'static [CliExample],
    pub removed: bool,
}
```

```rust
pub struct CliArgSpec {
    pub name: &'static str,
    pub value_type: CliValueType,
    pub required: bool,
    pub multiple: bool,
    pub maps_to_field: &'static str,
    pub help: &'static str,
}

pub struct CliFlagSpec {
    pub long: &'static str,
    pub short: Option<char>,
    pub value_type: CliValueType,
    pub enum_values: &'static [&'static str],
    pub required: bool,
    pub repeatable: bool,
    pub default: Option<&'static str>,
    pub maps_to_field: &'static str,
    pub env_override: Option<&'static str>,
    pub config_key: Option<&'static str>,
    pub help: &'static str,
}
```

The generated schema is the normalized command registry, not raw clap debug
output.

## Required Top-Level Commands

| Command | DTO | Service | Notes |
|---|---|---|---|
| `axon <source>` | `SourceRequest` | `SourceService.submit` | default source acquisition/indexing |
| `axon scrape <url>` | `SourceRequest` | `SourceService.submit` | `scope=page`, `embed=true`, exactly one page, clean content output |
| `axon watch <sub>` | `Watch*Request` | `WatchService` | durable watches |
| `axon map <source>` | `SourceRequest` | `SourceService.submit` | `intent=map`, `embed=false` |
| `axon extract <source>` | `ExtractRequest` | `ExtractService.extract` | structured LLM extraction |
| `axon search <query>` | `SearchRequest` | `SearchService.search` | external discovery |
| `axon query <query>` | `QueryRequest` | `QueryService.query` | indexed retrieval |
| `axon retrieve <source>` | `RetrievalRequest` | `RetrieveService.retrieve` | stored lookup |
| `axon ask <question>` | `AskRequest` | `AskService.ask` | RAG answer |
| `axon chat <message>` | `ChatRequest` | `ChatService.chat` | direct LLM chat |
| `axon evaluate <question>` | `EvaluationRequest` | `EvaluationService.evaluate` | RAG/baseline evaluation |
| `axon suggest [focus]` | `SuggestRequest` | `SuggestService.suggest` | source/acquisition suggestions |
| `axon research <query>` | `ResearchRequest` | `ResearchService.research` | web research synthesis |
| `axon summarize <source>` | `SummarizeRequest` | `SummarizeService.summarize` | fetch/scrape and summarize |
| `axon endpoints <source>` | `EndpointDiscoveryRequest` | `EndpointService.discover` | API/network endpoint discovery |
| `axon brand <source>` | `BrandRequest` | `BrandService.extract` | brand asset extraction |
| `axon diff <source-a> <source-b>` | `DiffRequest` | `DiffService.diff` | source comparison |
| `axon screenshot <source>` | `ScreenshotRequest` | `ScreenshotService.capture` | screenshot artifact capture |
| `axon memory <sub>` | `Memory*Request` | `MemoryService` | memory lifecycle |
| `axon graph <sub>` | `Graph*Request` | `GraphService` | graph query/resolve |
| `axon jobs <sub>` | `Job*Request` | `JobService` | job status/control |
| `axon artifacts <sub>` | `Artifact*Request` | `ArtifactService` | artifact listing/content |
| `axon uploads <sub>` | `Upload*Request` | `UploadService` | staged upload lifecycle |
| `axon providers <sub>` | `Provider*Request` | `ProviderService` | provider health/capabilities |
| `axon prune <sub>` | `Prune*Request` | `PruneService` | admin cleanup |
| `axon collections <sub>` | `Collection*Request` | `CollectionService` | vector collection inspection |
| `axon config <sub>` | `Config*Request` | `ConfigService` | config inspection/edit |
| `axon doctor` | `DoctorRequest` | `ProviderService.doctor` | diagnostics |
| `axon preflight` | `PreflightRequest` | `ProviderService.preflight` | config/provider readiness before work |
| `axon smoke` | `SmokeRequest` | `ProviderService.smoke` | explicit live smoke checks |
| `axon status` | `StatusRequest` | `ProviderService.status` | runtime status |
| `axon capabilities` | `CapabilityRequest` | `ProviderService.capabilities` | machine contract |

## Required Global Flags

| Flag | Maps To | Rules |
|---|---|---|
| `--json` | `ResponseMode`/output renderer | Emits JSON envelope; no prose mixed into stdout. |
| `--wait` | `ExecutionMode::Wait` | Blocks until terminal result when supported. |
| `--no-embed` | `SourceRequest.embed=false` | Valid only for source/map-capable flows. |
| `--watch` | `SourceRequest.watch=ensure` | Creates/ensures durable watch for source. |
| `--refresh` | `SourceRequest.refresh=force` | Forces refresh for source lifecycle. |
| `--scope <scope>` | `SourceRequest.scope` | Must be adapter-declared scope. |
| `--collection <name>` | source/vector request collection | Optional override; default from config. |
| `--adapter <name>` | `SourceRequest.adapter_hint` | Resolver must validate adapter supports source. |
| `--config <path>` | bootstrap config path | Env/bootstrap only; not persisted in DTO. |

Global flags must not apply silently to commands that cannot honor them. The
schema marks per-command allowed global flags.

## Parse Output Contract

CLI parsing produces:

```json
{
  "command_path": ["axon", "ask"],
  "request_dto": "AskRequest",
  "request": {},
  "execution": {
    "mode": "foreground",
    "json": false,
    "wait": false
  },
  "rendering": {
    "format": "human",
    "color": "auto"
  }
}
```

Rules:

- parser validation happens before side effects
- removed commands fail before service dispatch
- JSON mode writes only JSON to stdout
- human mode may write progress to stderr/stdout according to command contract
- command examples in docs must parse into expected DTO snapshots

## Removed Command Absence

These must not appear in `commands.json`, help output, completions, parser, or
aliases:

- `embed`
- `ingest`
- `crawl`
- `code-search`
- `code-search-watch`
- old `purge` alias paths

## Generated Completion Contract

Shell completions are generated from the same `CliCommandSpec`. Completion
snapshots must fail when command schema and completions diverge.

Required generated artifacts:

```text
target/generated-docs/completions/axon.bash
target/generated-docs/completions/axon.zsh
target/generated-docs/completions/axon.fish
```

## Drift Checks

Fail when:

- clap command exists but command contract omits it
- command contract lists command absent from clap
- generated help differs from `axon-help.md`
- removed command appears
- command maps to non-existent DTO
- flag default differs from config contract
- command example fails to parse
- global flag is accepted by a command that cannot honor it
- JSON mode emits non-JSON stdout in fixture tests

## Validation Fixtures

Required fixtures:

```text
crates/axon-cli/tests/fixtures/schema/source.valid.json
crates/axon-cli/tests/fixtures/schema/ask.valid.json
crates/axon-cli/tests/fixtures/schema/query.valid.json
crates/axon-cli/tests/fixtures/schema/removed-embed.invalid.json
crates/axon-cli/tests/fixtures/schema/watch-exec.valid.json
crates/axon-cli/tests/fixtures/schema/map-no-embed.valid.json
crates/axon-cli/tests/fixtures/schema/json-ask.valid.json
crates/axon-cli/tests/fixtures/schema/unknown-global.invalid.json
```

## Acceptance Criteria

- every canonical command has a `CliCommandSpec`
- every `CliCommandSpec.maps_to_dto` exists in `axon-api`
- generated `commands.json` and generated help come from the same registry
- removed commands are absent from schema, help, parser, and completions
- fixture commands parse into the expected DTOs
- completion snapshots match command schema
- every command example in `command-contract.md` and `axon-help.md` has a parse
  fixture or generated test
- removed command strings are absent from help, completions, schema, and parser
