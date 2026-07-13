//! Vector-payload source-family taxonomy: the allowed `source_family` values
//! and, per family, the source-specific metadata fields permitted in a vector
//! point payload. Extracted from `payload.rs` to keep that file under the
//! monolith cap and to give the family taxonomy a single obvious home as more
//! source families land.

pub const VECTOR_SOURCE_FAMILIES: &[&str] = &[
    "code", "web", "package", "session", "graph", "memory", "feed", "social", "media", "local",
    "tool", "docker", "env", "upload",
];

pub const VECTOR_SOURCE_FAMILY_FIELDS: &[(&str, &[&str])] = &[
    (
        "code",
        &[
            "code_language",
            "code_symbol_name",
            "code_symbol_kind",
            "code_file_type",
            "code_is_test",
            "code_parse_status",
            "symbol_extraction_status",
            "manifest",
            "git_provider",
            "git_host",
            "git_repo",
            "git_owner",
            "git_web_url",
        ],
    ),
    (
        "feed",
        &[
            "feed_title",
            "feed_link",
            "feed_entry_id",
            "feed_entry_link",
            "feed_entry_published",
            "feed_entry_author",
            "structured_parse_error",
        ],
    ),
    (
        "social",
        &[
            "reddit_author",
            "reddit_created_utc",
            "reddit_score",
            "reddit_num_comments",
            "reddit_upvote_ratio",
            "reddit_subreddit",
            "reddit_domain",
            "reddit_is_video",
            "reddit_distinguished",
            "reddit_gilded",
            "reddit_flair",
            "reddit_permalink",
            "reddit_kind",
        ],
    ),
    (
        "media",
        &[
            "video_id",
            "title",
            "url",
            "channel",
            "channel_url",
            "yt_uploader_id",
            "yt_upload_date",
            "yt_duration",
            "yt_view_count",
            "yt_like_count",
            "yt_tags",
            "yt_categories",
            "yt_thumbnail",
            "segment_kind",
        ],
    ),
    (
        "web",
        &[
            "web_title",
            "web_domain",
            "web_status_code",
            "web_depth",
            "normalization_version",
            "web_url",
            "web_seed_url",
            "web_origin",
            "web_path",
            "web_normalized_url",
            "web_fetch_method",
            "extractor_name",
            "extractor_version",
            "structured_payload_omitted",
            // Off-band structured-data extraction (JSON-LD / `__NEXT_DATA__` /
            // SvelteKit island) captured on `SourceDocument::structured_payload`
            // by the web source adapter and projected onto every chunk of the
            // document by `axon_document::preparer::project_structured_payload_metadata`
            // (markdown-routed web docs never otherwise touch that field --
            // see that function's doc comment). `web_structured_kind` is the
            // schema.org/JSON-LD type when known, else the coarser extraction
            // mechanism (`jsonld`/`next_data`/`sveltekit`); `web_structured_blob`
            // is the bounded (<=64 KiB) JSON-stringified payload.
            "web_structured_kind",
            "web_structured_blob",
        ],
    ),
    (
        "package",
        &["package_ecosystem", "package_name", "package_version"],
    ),
    (
        "session",
        &[
            "session_provider",
            "session_id",
            "session_turn_index",
            "session_tool_name",
            "session_skill_name",
        ],
    ),
    (
        "graph",
        &["graph_node_ids", "graph_edge_ids", "graph_confidence"],
    ),
    (
        "memory",
        &[
            "memory_id",
            "memory_importance",
            "memory_status",
            "memory_recallable",
            "memory_type",
            "memory_scope_kind",
            "memory_scope_value",
            "memory_confidence",
            "memory_salience",
            "redaction_version",
            "redacted_field_count",
            "dropped_field_count",
            "detector_names",
        ],
    ),
    (
        "local",
        &[
            "local_checkout",
            "local_path_key",
            "local_git_remote",
            "local_git_commit",
        ],
    ),
    (
        "tool",
        &[
            "tool_name",
            "tool_action",
            "tool_side_effect_class",
            "tool_output_artifact_id",
        ],
    ),
    (
        "docker",
        &[
            "docker_image",
            "docker_service",
            "docker_port",
            "docker_volume",
        ],
    ),
    // Not `env_secret_reference`/`env_value_*` -- "secret" and "env_value"
    // are both fatal `FORBIDDEN_FIELD_FRAGMENTS` substrings in the shared
    // redaction boundary (fail-closed by design), even though this field
    // only ever holds a locator/reference, never a secret value. Currently
    // declarative only -- no adapter reads or writes it yet.
    ("env", &["env_key", "env_locator"]),
    // `UploadSourceAdapter` (`crates/axon-adapters/src/upload.rs`) stamps
    // `source_family = "upload"` on every normalized document -- distinct
    // from `local`/`code` because uploaded content is provenance-tracked
    // separately from a caller-specified local path (see that adapter's
    // module doc). `staged_upload` is the only source-specific field it
    // emits; every other field it writes (`source_kind`, `source_adapter`,
    // `source_scope`, `item_canonical_uri`, ...) is already a
    // `VECTOR_SHARED_FIELDS` member. Resolves the gap documented in
    // `docs/pipeline-unification/sources/metadata-payload.md`'s "Source
    // Family Classification" section.
    ("upload", &["staged_upload"]),
];
