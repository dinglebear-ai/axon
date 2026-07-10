use axon_api::source::{ContentKind, LifecycleStatus};

use crate::parser::{ParseInput, ParseResult, ParserCapability, SourceParser, stage_header};
use crate::registry::ParserRegistry;
use crate::{code, docker, env, manifest, markdown, schema, session, tool, tool_schema};

pub fn production_registry() -> ParserRegistry {
    ParserRegistry::new()
        .with_parser(SchemaParser)
        .with_parser(DockerManifestParser)
        .with_parser(EnvExampleParser)
        .with_parser(CodeSymbolsParser)
        .with_parser(ManifestParser)
        .with_parser(MarkdownParser)
        .with_parser(ToolParser)
        .with_parser(ToolSchemaParser)
        .with_parser(SessionParser)
}

struct CodeSymbolsParser;
struct ManifestParser;
struct MarkdownParser;
struct SchemaParser;
struct DockerManifestParser;
struct EnvExampleParser;
struct SessionParser;
struct ToolParser;
struct ToolSchemaParser;

impl SourceParser for CodeSymbolsParser {
    fn capability(&self) -> &ParserCapability {
        static CAPABILITY: std::sync::OnceLock<ParserCapability> = std::sync::OnceLock::new();
        CAPABILITY.get_or_init(|| ParserCapability {
            parser_id: "code_symbols".to_string(),
            parser_version: crate::facts::PARSER_VERSION.to_string(),
            content_kinds: vec![ContentKind::Code],
            mime_types: Vec::new(),
            file_extensions: vec![
                "rs".to_string(),
                "py".to_string(),
                "ts".to_string(),
                "tsx".to_string(),
                "js".to_string(),
                "jsx".to_string(),
            ],
            path_suffixes: Vec::new(),
            sniff_prefixes: Vec::new(),
            priority: 20,
        })
    }

    fn parse(&self, input: &ParseInput) -> ParseResult {
        let (facts, graph_candidates) = code::symbol_facts_with_graph(input);
        // No tree-sitter/AST grammar is wired in yet (see code.rs); every
        // fact here is a `regex_fallback` line/indentation heuristic, so the
        // fallback must be visible per parsing-contract.md, not just implied
        // by the per-fact `parser_method`/`confidence`.
        if facts.is_empty() {
            return completed_result(input, self.capability(), facts, graph_candidates);
        }
        let warning = axon_api::source::SourceWarning {
            code: "parse.code_ast_unavailable".to_string(),
            severity: axon_api::source::Severity::Info,
            message: "no AST/tree-sitter grammar is wired into axon-parse yet; code symbols \
                      were extracted with a line/indentation regex_fallback heuristic"
                .to_string(),
            source_item_key: Some(input.document.source_item_key.clone()),
            retryable: false,
        };
        ParseResult {
            header: stage_header(
                input,
                LifecycleStatus::CompletedDegraded,
                vec![warning.clone()],
                None,
            ),
            document_id: input.document.document_id.clone(),
            facts,
            graph_candidates,
            parser_id: self.capability().parser_id.clone(),
            parser_version: self.capability().parser_version.clone(),
            warnings: vec![warning],
            errors: Vec::new(),
        }
    }
}

impl SourceParser for ManifestParser {
    fn capability(&self) -> &ParserCapability {
        static CAPABILITY: std::sync::OnceLock<ParserCapability> = std::sync::OnceLock::new();
        CAPABILITY.get_or_init(|| ParserCapability {
            parser_id: "manifest".to_string(),
            parser_version: crate::facts::PARSER_VERSION.to_string(),
            content_kinds: vec![
                ContentKind::Toml,
                ContentKind::Json,
                ContentKind::Yaml,
                ContentKind::Xml,
                ContentKind::PlainText,
            ],
            mime_types: Vec::new(),
            file_extensions: Vec::new(),
            path_suffixes: vec![
                "Cargo.toml".to_string(),
                "package.json".to_string(),
                "requirements.txt".to_string(),
                "pyproject.toml".to_string(),
                "go.mod".to_string(),
                "pom.xml".to_string(),
                ".yaml".to_string(),
                ".yml".to_string(),
            ],
            sniff_prefixes: Vec::new(),
            priority: 10,
        })
    }

    fn parse(&self, input: &ParseInput) -> ParseResult {
        manifest::dependency_parse_result(input)
    }
}

impl SourceParser for MarkdownParser {
    fn capability(&self) -> &ParserCapability {
        static CAPABILITY: std::sync::OnceLock<ParserCapability> = std::sync::OnceLock::new();
        CAPABILITY.get_or_init(|| ParserCapability {
            parser_id: "markdown_headings".to_string(),
            parser_version: crate::facts::PARSER_VERSION.to_string(),
            content_kinds: vec![ContentKind::Markdown],
            mime_types: vec!["text/markdown".to_string()],
            file_extensions: vec!["md".to_string(), "mdx".to_string()],
            path_suffixes: Vec::new(),
            sniff_prefixes: vec!["#".to_string()],
            priority: 30,
        })
    }

    fn parse(&self, input: &ParseInput) -> ParseResult {
        let (facts, graph_candidates) = markdown::heading_facts(input);
        completed_result(input, self.capability(), facts, graph_candidates)
    }
}

impl SourceParser for SchemaParser {
    fn capability(&self) -> &ParserCapability {
        static CAPABILITY: std::sync::OnceLock<ParserCapability> = std::sync::OnceLock::new();
        CAPABILITY.get_or_init(|| ParserCapability {
            parser_id: "api_schema".to_string(),
            parser_version: crate::facts::PARSER_VERSION.to_string(),
            content_kinds: vec![ContentKind::Json, ContentKind::Yaml, ContentKind::PlainText],
            mime_types: vec![
                "application/schema+json".to_string(),
                "application/graphql".to_string(),
                "application/protobuf".to_string(),
            ],
            file_extensions: vec![
                "graphql".to_string(),
                "graphqls".to_string(),
                "proto".to_string(),
            ],
            path_suffixes: vec![
                "openapi.json".to_string(),
                "openapi.yaml".to_string(),
                "swagger.json".to_string(),
                "swagger.yaml".to_string(),
            ],
            sniff_prefixes: vec!["{\"openapi\"".to_string(), "type Query".to_string()],
            priority: 4,
        })
    }

    fn parse(&self, input: &ParseInput) -> ParseResult {
        let (facts, graph_candidates) = schema::api_schema_facts(input);
        completed_result(input, self.capability(), facts, graph_candidates)
    }
}

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

impl SourceParser for SessionParser {
    fn capability(&self) -> &ParserCapability {
        static CAPABILITY: std::sync::OnceLock<ParserCapability> = std::sync::OnceLock::new();
        CAPABILITY.get_or_init(|| ParserCapability {
            parser_id: "session_jsonl".to_string(),
            parser_version: crate::facts::PARSER_VERSION.to_string(),
            content_kinds: vec![ContentKind::Transcript],
            mime_types: Vec::new(),
            file_extensions: vec!["jsonl".to_string()],
            path_suffixes: vec!["session.jsonl".to_string()],
            sniff_prefixes: Vec::new(),
            priority: 5,
        })
    }

    fn parse(&self, input: &ParseInput) -> ParseResult {
        session::session_parse_result(input)
    }
}

impl SourceParser for ToolParser {
    fn capability(&self) -> &ParserCapability {
        static CAPABILITY: std::sync::OnceLock<ParserCapability> = std::sync::OnceLock::new();
        CAPABILITY.get_or_init(|| ParserCapability {
            parser_id: "tool_output_jsonl".to_string(),
            parser_version: crate::facts::PARSER_VERSION.to_string(),
            content_kinds: vec![ContentKind::Structured],
            mime_types: Vec::new(),
            file_extensions: Vec::new(),
            path_suffixes: vec!["tool-output.jsonl".to_string()],
            sniff_prefixes: vec!["{\"tool\"".to_string()],
            priority: 4,
        })
    }

    fn parse(&self, input: &ParseInput) -> ParseResult {
        tool::tool_parse_result(input)
    }
}

impl SourceParser for ToolSchemaParser {
    fn capability(&self) -> &ParserCapability {
        static CAPABILITY: std::sync::OnceLock<ParserCapability> = std::sync::OnceLock::new();
        CAPABILITY.get_or_init(|| ParserCapability {
            parser_id: "tool_schema".to_string(),
            parser_version: crate::facts::PARSER_VERSION.to_string(),
            content_kinds: Vec::new(),
            mime_types: Vec::new(),
            file_extensions: Vec::new(),
            path_suffixes: vec![
                "--help.txt".to_string(),
                "cli-help.txt".to_string(),
                "mcp-tools-list.json".to_string(),
                "mcp.tools.json".to_string(),
            ],
            sniff_prefixes: vec![
                "Usage:".to_string(),
                "USAGE:".to_string(),
                "{\"tools\":".to_string(),
            ],
            priority: 4,
        })
    }

    fn parse(&self, input: &ParseInput) -> ParseResult {
        let parsed = tool_schema::tool_schema_parse_items(input);
        completed_result(
            input,
            self.capability(),
            parsed.facts,
            parsed.graph_candidates,
        )
    }
}

fn completed_result(
    input: &ParseInput,
    capability: &ParserCapability,
    facts: Vec<axon_api::source::SourceParseFacts>,
    graph_candidates: Vec<axon_api::source::GraphCandidate>,
) -> ParseResult {
    ParseResult {
        header: stage_header(input, LifecycleStatus::Completed, Vec::new(), None),
        document_id: input.document.document_id.clone(),
        facts,
        graph_candidates,
        parser_id: capability.parser_id.clone(),
        parser_version: capability.parser_version.clone(),
        warnings: Vec::new(),
        errors: Vec::new(),
    }
}
