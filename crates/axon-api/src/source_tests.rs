use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::*;

#[test]
fn local_file_request_serializes_to_contract_shape() {
    let request = SourceRequest::local_path("/tmp/example.md", false);
    let value = serde_json::to_value(&request).expect("serialize source request");

    assert_eq!(value["source"], "/tmp/example.md");
    assert_eq!(value["intent"], "acquire");
    assert_eq!(value["embed"], true);
    assert_eq!(value["refresh"], "if_stale");
    assert_eq!(value["watch"], "disabled");
    assert_eq!(value["execution"]["mode"], "background");
    assert_eq!(value["execution"]["priority"], "normal");
    assert_eq!(value["output"]["response_mode"], "auto");
    assert_eq!(value["output"]["artifact_mode"], "on_large_output");
    assert_eq!(value["scope"], "file");
    assert_eq!(value["adapter"], "local");
}

#[test]
fn source_request_deserializes_with_defaults_for_minimal_input() {
    let request: SourceRequest =
        serde_json::from_value(json!({ "source": "shadcn.com" })).expect("minimal request");

    assert_eq!(request.source, "shadcn.com");
    assert_eq!(request.intent, SourceIntent::Acquire);
    assert_eq!(request.refresh, SourceRefreshPolicy::IfStale);
    assert_eq!(request.watch, SourceWatchPolicy::Disabled);
    assert_eq!(request.execution.mode, ExecutionMode::Background);
    assert_eq!(request.execution.priority, JobPriority::Normal);
    assert!(request.embed);
    assert!(request.options.values.is_empty());
    assert!(request.metadata.is_empty());
}

#[test]
fn resolved_source_serializes_to_api_contract_shape() {
    let resolved = ResolvedSource {
        source: "https://example.com/docs".to_string(),
        canonical_uri: "https://example.com/docs".to_string(),
        source_id: SourceId::from("src_web_example_docs"),
        source_kind: SourceKind::Web,
        adapter: AdapterRef {
            name: "web".to_string(),
            version: "1".to_string(),
        },
        default_scope: SourceScope::Site,
        available_scopes: vec![SourceScope::Page, SourceScope::Site, SourceScope::Map],
        authority: AuthorityLevel::Official,
        confidence: 0.97,
        reason: "canonical https URL".to_string(),
        graph: Vec::new(),
        warnings: Vec::new(),
        metadata: MetadataMap::new(),
    };

    let value = serde_json::to_value(&resolved).expect("serialize resolved source");

    assert_eq!(value["source"], "https://example.com/docs");
    assert_eq!(value["canonical_uri"], "https://example.com/docs");
    assert_eq!(value["source_kind"], "web");
    assert_eq!(value["adapter"]["name"], "web");
    assert_eq!(value["default_scope"], "site");
    assert_eq!(value["available_scopes"], json!(["page", "site", "map"]));
    assert!(value.get("graph").is_none() || value["graph"].as_array().is_some());
    assert!(value.get("requested_uri").is_none());
    assert!(value.get("candidate_adapters").is_none());
    assert!(value.get("display_name").is_none());
}

#[test]
fn source_request_rejects_unknown_external_fields() {
    let err = serde_json::from_value::<SourceRequest>(json!({
        "source": "github.com/jmagar/axon",
        "old_embed_request_field": true
    }))
    .expect_err("unknown fields must fail");

    assert!(err.to_string().contains("unknown field"), "{err}");
}

#[test]
fn source_request_rejects_removed_flat_execution_output_shape() {
    let err = serde_json::from_value::<SourceRequest>(json!({
        "source": "github.com/jmagar/axon",
        "execution": "background",
        "output": "auto"
    }))
    .expect_err("clean-break source request must reject old flat policy fields");

    assert!(err.to_string().contains("invalid type"), "{err}");
}

#[test]
fn nested_source_dtos_reject_unknown_fields() {
    let content_err = serde_json::from_value::<ContentRef>(json!({
        "kind": "artifact",
        "artifact_id": "artifact_1",
        "extra": true
    }))
    .expect_err("content ref must reject unknown fields");
    assert!(
        content_err.to_string().contains("unknown field"),
        "{content_err}"
    );

    let selector_err = serde_json::from_value::<CleanupSelector>(json!({
        "kind": "generation",
        "source_id": "src_local_workspace",
        "generation": "gen_0001",
        "extra": true
    }))
    .expect_err("cleanup selector must reject unknown fields");
    assert!(
        selector_err.to_string().contains("unknown field"),
        "{selector_err}"
    );
}

#[test]
fn enum_wire_values_are_snake_case_and_closed() {
    assert_eq!(
        serde_json::to_value(SourceKind::McpTool).unwrap(),
        json!("mcp_tool")
    );
    assert_eq!(
        serde_json::to_value(SourceScope::PullRequest).unwrap(),
        json!("pull_request")
    );
    assert_eq!(
        serde_json::to_value(PipelinePhase::Vectorizing).unwrap(),
        json!("vectorizing")
    );
    assert_eq!(
        serde_json::to_value(PublishState::CleanupPending).unwrap(),
        json!("cleanup_pending")
    );
    assert_eq!(
        serde_json::to_value(LifecycleStatus::CompletedDegraded).unwrap(),
        json!("completed_degraded")
    );
    assert_eq!(
        serde_json::to_value(CleanupDebtKind::VectorDelete).unwrap(),
        json!("vector_delete")
    );
    assert_eq!(
        serde_json::to_value(DocumentLifecycleStatus::Vectorized).unwrap(),
        json!("vectorized")
    );
    assert_eq!(
        serde_json::to_value(CleanupSelector::SourceItem {
            source_id: SourceId::from("src_local_workspace"),
            source_item_key: SourceItemKey::from("src/lib.rs"),
            generation: SourceGenerationId::from("gen_0001"),
        })
        .unwrap(),
        json!({
            "kind": "source_item",
            "source_id": "src_local_workspace",
            "source_item_key": "src/lib.rs",
            "generation": "gen_0001",
        })
    );

    let err = serde_json::from_value::<SourceKind>(json!("mystery_source"))
        .expect_err("unknown source kind must fail");
    assert!(err.to_string().contains("unknown variant"), "{err}");
}

#[test]
fn force_refresh_and_watch_helpers_set_intent_without_disabling_embedding() {
    let refresh =
        SourceRequest::new("github.com/jmagar/axon").with_refresh(SourceRefreshPolicy::Force);
    assert_eq!(refresh.intent, SourceIntent::Refresh);
    assert_eq!(refresh.refresh, SourceRefreshPolicy::Force);
    assert!(refresh.embed);

    let watch =
        SourceRequest::local_path("/workspace/axon", true).with_watch(SourceWatchPolicy::Ensure);
    assert_eq!(watch.intent, SourceIntent::Watch);
    assert_eq!(watch.watch, SourceWatchPolicy::Ensure);
    assert_eq!(watch.scope, Some(SourceScope::Directory));
    assert!(watch.embed);
}

#[test]
fn stage_execution_result_wraps_payload_with_required_header() {
    let header = StageResultHeader {
        job_id: JobId(Uuid::new_v4()),
        stage_id: StageId(Uuid::new_v4()),
        phase: PipelinePhase::Resolving,
        status: LifecycleStatus::Completed,
        started_at: Timestamp::from(Utc::now()),
        completed_at: Some(Timestamp::from(Utc::now())),
        counts: StageCounts {
            items_total: Some(1),
            items_done: 1,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    };
    let result = StageExecutionResult {
        header,
        data: SourceRequest::new("shadcn.com"),
    };
    let value = serde_json::to_value(&result).expect("stage result");

    assert_eq!(value["header"]["phase"], "resolving");
    assert_eq!(value["header"]["status"], "completed");
    assert_eq!(value["data"]["source"], "shadcn.com");
}

#[test]
fn concrete_stage_results_round_trip() {
    let header = StageResultHeader {
        job_id: JobId(Uuid::new_v4()),
        stage_id: StageId(Uuid::new_v4()),
        phase: PipelinePhase::Authorizing,
        status: LifecycleStatus::Completed,
        started_at: Timestamp::from(Utc::now()),
        completed_at: Some(Timestamp::from(Utc::now())),
        counts: StageCounts {
            items_total: None,
            items_done: 0,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    };
    let auth = AuthorizationResult {
        header: header.clone(),
        source_id: Some(SourceId::from("src_local")),
        decision: SecurityDecision {
            allowed: true,
            scope: "source:write".to_string(),
            reason: "test caller".to_string(),
            redactions: Vec::new(),
            warnings: Vec::new(),
        },
        caller: CallerContext {
            actor: Some("cli".to_string()),
            transport: TransportKind::Cli,
            scopes: vec!["source:write".to_string()],
            visibility_ceiling: Visibility::Internal,
        },
    };
    let write = VectorStoreWriteResult {
        header: StageResultHeader {
            phase: PipelinePhase::Upserting,
            ..header.clone()
        },
        collection: "axon".to_string(),
        points_attempted: 2,
        points_written: 2,
        payload_indexes_created: vec!["source_id".to_string()],
        usage: ProviderUsage {
            input_tokens: Some(100),
            output_tokens: None,
            requests: 1,
            duration_ms: 42,
        },
    };
    let publish = PublishGenerationResult {
        header: StageResultHeader {
            phase: PipelinePhase::Publishing,
            ..header
        },
        source_id: SourceId::from("src_local"),
        generation: SourceGenerationId::from("gen_0002"),
        published_at: Timestamp::from(Utc::now()),
        document_count: 1,
        chunk_count: 2,
        vector_point_count: 2,
        cleanup_debt: vec![CleanupDebtId::from("debt_1")],
    };

    assert_eq!(
        serde_json::from_value::<AuthorizationResult>(serde_json::to_value(&auth).unwrap())
            .unwrap(),
        auth
    );
    assert_eq!(
        serde_json::from_value::<VectorStoreWriteResult>(serde_json::to_value(&write).unwrap())
            .unwrap(),
        write
    );
    assert_eq!(
        serde_json::from_value::<PublishGenerationResult>(serde_json::to_value(&publish).unwrap())
            .unwrap(),
        publish
    );
}

#[test]
fn source_document_and_prepared_document_carry_generation_identity() {
    let doc = SourceDocument {
        document_id: DocumentId::from("doc_local_readme"),
        source_id: SourceId::from("src_local_workspace"),
        source_item_key: SourceItemKey::from("README.md"),
        canonical_uri: "file:///workspace/README.md".to_string(),
        content_kind: ContentKind::Markdown,
        content: ContentRef::InlineText {
            text: "# Axon".to_string(),
        },
        metadata: MetadataMap::new(),
        title: Some("Axon".to_string()),
        language: None,
        path: Some("README.md".to_string()),
        mime_type: Some("text/markdown".to_string()),
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    };
    let prepared = PreparedDocument {
        document_id: doc.document_id.clone(),
        source_id: doc.source_id.clone(),
        source_item_key: doc.source_item_key.clone(),
        generation: SourceGenerationId::from("gen_0001"),
        canonical_uri: doc.canonical_uri.clone(),
        prepare_version: "axon-document@1".to_string(),
        chunking_profile: "markdown_sections".to_string(),
        chunking_method: "markdown_splitter".to_string(),
        chunks: Vec::new(),
        metadata: MetadataMap::new(),
        cleanup_keys: Vec::new(),
        graph_refs: Vec::new(),
        parse_facts: Vec::new(),
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    assert_eq!(prepared.source_id, doc.source_id);
    assert_eq!(prepared.source_item_key, SourceItemKey::from("README.md"));
    assert_eq!(prepared.generation, SourceGenerationId::from("gen_0001"));
}

#[test]
fn pr8_prepared_document_contract_accepts_parse_graph_and_chunk_metadata() {
    let value: serde_json::Value = serde_json::from_str(
        r##"{
        "document_id": "doc_local_readme",
        "source_id": "src_local_workspace",
        "source_item_key": "README.md",
        "generation": "gen_0001",
        "canonical_uri": "file:///workspace/README.md",
        "prepare_version": "axon-document@1",
        "chunking_profile": "markdown_sections",
        "chunking_method": "markdown_splitter",
        "chunks": [{
            "chunk_id": "chunk_readme_0001",
            "chunk_key": "src_local_workspace/gen_0001/README.md/markdown_sections/0",
            "document_id": "doc_local_readme",
            "chunk_index": 0,
            "content": "# Axon",
            "embedding_text": "Axon",
            "content_hash": "sha256:readmechunk",
            "chunk_locator": {
                "canonical_uri": "file:///workspace/README.md",
                "path": "README.md",
                "heading_path": ["Axon"],
                "range": {
                    "line_start": 1,
                    "line_end": 1,
                    "byte_start": 0,
                    "byte_end": 6,
                    "char_start": 0,
                    "char_end": 6
                }
            },
            "source_range": {
                "line_start": 1,
                "line_end": 1,
                "byte_start": 0,
                "byte_end": 6,
                "char_start": 0,
                "char_end": 6
            },
            "content_kind": "markdown",
            "title": "Axon",
            "graph_refs": [{ "candidate_id": "cand_heading_axon" }],
            "metadata": {
                "heading_level": 1
            }
        }],
        "metadata": {},
        "cleanup_keys": [],
        "graph_refs": [{ "candidate_id": "cand_heading_axon" }],
        "parse_facts": [{
            "document_id": "doc_local_readme",
            "source_item_key": "README.md",
            "fact_kind": "heading",
            "name": "Axon",
            "value": { "level": 1 },
            "parser_id": "markdown",
            "parser_version": "1",
            "parser_method": "markdown_ast",
            "range": {
                "line_start": 1,
                "line_end": 1,
                "byte_start": 0,
                "byte_end": 6,
                "char_start": 0,
                "char_end": 6
            },
            "confidence": 1.0,
            "metadata": {}
        }],
        "graph_candidates": [{
            "candidate_id": "cand_heading_axon",
            "job_id": "00000000-0000-0000-0000-000000000001",
            "source_id": "src_local_workspace",
            "source_item_key": "README.md",
            "item_canonical_uri": "file:///workspace/README.md",
            "document_id": "doc_local_readme",
            "kind": "document_heading",
            "merge_key": "heading:file:///workspace/README.md#Axon",
            "producer": {
                "adapter": "local",
                "parser": "markdown",
                "version": "1"
            },
            "nodes": [],
            "edges": [],
            "evidence": [{
                "evidence_id": "ev_heading_axon",
                "evidence_kind": "chunk_range",
                "source_id": "src_local_workspace",
                "source_item_key": "README.md",
                "document_id": "doc_local_readme",
                "chunk_id": "chunk_readme_0001",
                "range": {
                    "line_start": 1,
                    "line_end": 1,
                    "byte_start": 0,
                    "byte_end": 6,
                    "char_start": 0,
                    "char_end": 6
                },
                "quote": "# Axon",
                "confidence": 1.0,
                "metadata": {}
            }],
            "confidence": 1.0,
            "metadata": {}
        }],
        "warnings": [],
        "errors": []
    }"##,
    )
    .expect("prepared document fixture JSON");

    let prepared: PreparedDocument =
        serde_json::from_value(value).expect("PR8 prepared document contract should deserialize");

    assert_eq!(prepared.document_id, DocumentId::from("doc_local_readme"));
    assert_eq!(prepared.chunks.len(), 1);
}

#[test]
fn pr8_parse_result_contract_carries_stage_local_warnings_and_errors() {
    let value = json!({
        "header": {
            "job_id": "00000000-0000-0000-0000-000000000001",
            "stage_id": "00000000-0000-0000-0000-000000000002",
            "phase": "parsing",
            "status": "completed_degraded",
            "started_at": "2026-07-01T16:20:00Z",
            "completed_at": "2026-07-01T16:20:01Z",
            "counts": {
                "items_done": 1,
                "documents_done": 1,
                "chunks_done": 0,
                "bytes_done": 6
            },
            "warnings": [],
            "error": null
        },
        "document_id": "doc_local_readme",
        "facts": [],
        "graph_candidates": [],
        "parser_id": "markdown",
        "parser_version": "1",
        "warnings": [{
            "code": "parse.markdown.frontmatter_ignored",
            "severity": "warning",
            "message": "frontmatter was preserved as content",
            "source_item_key": "README.md",
            "retryable": false
        }],
        "errors": []
    });

    let result: ParseResult =
        serde_json::from_value(value).expect("PR8 parse result contract should deserialize");

    assert_eq!(result.document_id, DocumentId::from("doc_local_readme"));
}

#[test]
fn vector_payload_uses_contract_field_names() {
    let batch = VectorPointBatch {
        batch_id: BatchId(Uuid::new_v4()),
        collection: "axon".to_string(),
        points: vec![VectorPoint {
            point_id: VectorPointId::from("point_1"),
            chunk_id: ChunkId::from("chunk_1"),
            vector: vec![0.1, 0.2],
            sparse_vector: Some(SparseVector {
                chunk_id: ChunkId::from("chunk_1"),
                indices: vec![1, 4],
                values: vec![0.3, 0.7],
            }),
            payload: MetadataMap::new(),
        }],
        model: "Qwen3-Embedding-0.6B".to_string(),
        dimensions: 2,
        sparse_vectors: None,
        payload_indexes: vec![PayloadIndexSpec {
            field_name: "source_id".to_string(),
            field_schema: PayloadFieldSchema::Keyword,
            required_for_filters: true,
        }],
    };

    let value = serde_json::to_value(&batch).expect("vector point batch");
    assert_eq!(value["points"][0]["vector"].as_array().unwrap().len(), 2);
    assert_eq!(
        value["points"][0]["sparse_vector"]["indices"],
        json!([1, 4])
    );
    assert_eq!(value["payload_indexes"][0]["field_schema"], "keyword");
    assert!(value.get("sparse_vectors").is_none());
}

#[test]
fn source_generation_and_cleanup_debt_round_trip() {
    let generation = SourceGeneration {
        source_id: SourceId::from("src_local_workspace"),
        generation: SourceGenerationId::from("gen_0002"),
        status: LifecycleStatus::Running,
        publish_state: PublishState::Writing,
        created_at: Timestamp::from(Utc::now()),
        published_at: None,
        item_counts: ItemCounts {
            added: 1,
            modified: 2,
            removed: 0,
            unchanged: 4,
            failed: 0,
        },
        document_counts: DocumentCounts {
            discovered: 7,
            prepared: 3,
            embedded: 3,
            published: 0,
            failed: 0,
        },
        cleanup_debt: vec![CleanupDebtId::from("debt_1")],
        previous_generation: Some(SourceGenerationId::from("gen_0001")),
    };
    let debt = CleanupDebt {
        debt_id: CleanupDebtId::from("debt_1"),
        job_id: JobId(Uuid::new_v4()),
        source_id: generation.source_id.clone(),
        generation: Some(generation.generation.clone()),
        kind: CleanupDebtKind::VectorDelete,
        selector: CleanupSelector::Generation {
            source_id: generation.source_id.clone(),
            generation: SourceGenerationId::from("gen_0001"),
        },
        status: LifecycleStatus::Pending,
        created_at: Timestamp::from(Utc::now()),
        attempts: 0,
        last_error: None,
        next_retry_at: None,
        completed_at: None,
    };

    assert_eq!(
        serde_json::from_value::<SourceGeneration>(serde_json::to_value(&generation).unwrap())
            .unwrap(),
        generation
    );
    assert_eq!(
        serde_json::from_value::<CleanupDebt>(serde_json::to_value(&debt).unwrap()).unwrap(),
        debt
    );
}

#[test]
fn capability_document_uses_closed_provider_enums() {
    let capability = CapabilityDocument {
        server: ServerInfo {
            name: "axon".to_string(),
            version: "6.2.1".to_string(),
            build: None,
            environment: Some("test".to_string()),
        },
        generated_at: Timestamp::from(Utc::now()),
        source_kinds: vec![SourceKind::Local, SourceKind::Git],
        source_scopes: vec![SourceScope::File, SourceScope::Repo],
        pipeline_phases: vec![PipelinePhase::Resolving, PipelinePhase::Embedding],
        adapters: vec![SourceAdapterCapability::from(CapabilityBase {
            name: "local".to_string(),
            version: "0.1.0".to_string(),
            owner_crate: "axon-adapters".to_string(),
            health: HealthStatus::Healthy,
            features: vec!["manifest".to_string()],
            limits: MetadataMap::new(),
        })],
        providers: vec![ProviderCapability {
            provider_id: ProviderId::from("tei-default"),
            provider_kind: ProviderKind::Embedding,
            implementation: "tei".to_string(),
            version: "0.1.0".to_string(),
            health: HealthStatus::Healthy,
            features: vec!["dense".to_string()],
            limits: ProviderLimits {
                max_concurrency: Some(4),
                ..ProviderLimits::default()
            },
            cooldown_until: None,
            last_error: None,
            reservation_policy: ReservationPolicy {
                supports_reservations: true,
                queue_policy: QueuePolicy::Priority,
                interactive_reserve: 1,
                cooldown_after_failures: 3,
                cooldown_secs: 60,
                retry_backoff_ms: Some(250),
            },
            reservation_state: ReservationStateSnapshot {
                queued: 0,
                active: 0,
                available_units: 4,
                oldest_queued_ms: None,
                priority_breakdown: std::collections::BTreeMap::new(),
                states: vec![ReservationState::Granted],
            },
            cost_class: ProviderCostClass::Internal,
            degraded_modes: Vec::new(),
            fake_overrides_supported: true,
            embedding: Some(EmbeddingProviderCapability {
                model_id: "Qwen/Qwen3-Embedding-0.6B".to_string(),
                dimensions: 1024,
                max_input_tokens: 32_768,
                max_batch_tokens: 262_144,
                instruction_support: InstructionSupport::QueryAndDocument,
                sparse_output: false,
                batch_limits: BatchLimits {
                    max_items: 32,
                    max_tokens: 262_144,
                    max_bytes: None,
                },
            }),
            llm: None,
            vector_store: None,
            fetch: None,
            render: None,
            credential: None,
        }],
        stores: StoreCapabilities {
            ledger: None,
            graph: None,
            memory: None,
            job: None,
            watch: None,
            artifact: None,
            config: None,
            document_cache: None,
        },
        metadata: MetadataMap::new(),
    };

    let value = serde_json::to_value(&capability).expect("capability document");
    assert_eq!(value["providers"][0]["provider_kind"], "embedding");
    assert_eq!(value["providers"][0]["implementation"], "tei");
    assert_eq!(value["adapters"][0]["owner_crate"], "axon-adapters");
}

#[path = "source_tests/document_cache_tests.rs"]
mod document_cache_tests;
