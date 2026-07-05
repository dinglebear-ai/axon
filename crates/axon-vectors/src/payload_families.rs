//! Vector-payload source-family taxonomy: the allowed `source_family` values
//! and, per family, the source-specific metadata fields permitted in a vector
//! point payload. Extracted from `payload.rs` to keep that file under the
//! monolith cap and to give the family taxonomy a single obvious home as more
//! source families land.

pub const VECTOR_SOURCE_FAMILIES: &[&str] = &[
    "code", "web", "package", "session", "graph", "memory", "feed", "social", "media",
];

pub const VECTOR_SOURCE_FAMILY_FIELDS: &[(&str, &[&str])] = &[
    (
        "code",
        &[
            "code_language",
            "code_symbol_name",
            "code_symbol_kind",
            "code_file_type",
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
        &["web_title", "web_domain", "web_status_code", "web_depth"],
    ),
    (
        "package",
        &["package_ecosystem", "package_name", "package_version"],
    ),
    (
        "session",
        &[
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
        &["memory_id", "memory_importance", "memory_status"],
    ),
];
