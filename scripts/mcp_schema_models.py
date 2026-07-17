"""Data structures and constants for MCP schema documentation generation.

Contains the parsed representations of Rust structs/enums and the
action-name mappings derived from the AxonRequest serde contract.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field


# ---------------------------------------------------------------------------
# Data structures
# ---------------------------------------------------------------------------


@dataclass
class FieldDef:
    """A single struct field parsed from Rust source."""

    name: str
    rust_type: str
    aliases: list[str] = field(default_factory=list)

    @property
    def is_optional(self) -> bool:
        return self.rust_type.startswith("Option<")

    @property
    def inner_type(self) -> str:
        """Unwrap Option<T> -> T, otherwise return raw type."""
        m = re.match(r"Option<(.+)>", self.rust_type)
        return m.group(1) if m else self.rust_type

    @property
    def display_type(self) -> str:
        """Human-readable type for docs."""
        inner = self.inner_type
        type_map: dict[str, str] = {
            "String": "string",
            "bool": "bool",
            "u32": "u32",
            "u64": "u64",
            "i64": "i64",
            "usize": "usize",
            "Vec<String>": "string[]",
            "Value": "any",
        }
        return type_map.get(inner, inner)


@dataclass
class StructDef:
    """A parsed Rust struct."""

    name: str
    fields: list[FieldDef] = field(default_factory=list)

    @property
    def has_subaction(self) -> bool:
        return any(f.name == "subaction" for f in self.fields)

    @property
    def subaction_enum_name(self) -> str | None:
        for f in self.fields:
            if f.name == "subaction":
                inner = f.inner_type
                if inner == "String":
                    return None
                return inner
        return None

    def optional_fields(self) -> list[FieldDef]:
        return [f for f in self.fields if f.is_optional]

    def required_fields(self) -> list[FieldDef]:
        return [f for f in self.fields if not f.is_optional and f.name != "subaction"]


@dataclass
class EnumDef:
    """A parsed Rust enum."""

    name: str
    variants: list[str] = field(default_factory=list)

    def snake_variants(self) -> list[str]:
        """Convert PascalCase variants to snake_case (matching serde rename_all)."""
        result: list[str] = []
        for v in self.variants:
            snake = re.sub(r"(?<=[a-z0-9])([A-Z])", r"_\1", v).lower()
            result.append(snake)
        return result


# ---------------------------------------------------------------------------
# Action name mapping (serde tag values from AxonRequest enum)
# ---------------------------------------------------------------------------

# Maps AxonRequest variant -> action name (snake_case from serde rename_all).
# Also maps Request struct name -> action name for struct lookups.
# MCP action surface. The legacy indexing actions (crawl/scrape/embed/ingest/
# code_search/vertical_scrape) were folded into the single `source` action and
# are intentionally absent here — they no longer exist on the MCP tool.
VARIANT_TO_ACTION: dict[str, str] = {
    "Status": "status",
    "Source": "source",
    "Extract": "extract",
    "Memory": "memory",
    "Query": "query",
    "Retrieve": "retrieve",
    "Search": "search",
    "Map": "map",
    "Endpoints": "endpoints",
    "Evaluate": "evaluate",
    "Suggest": "suggest",
    "Doctor": "doctor",
    "Domains": "domains",
    "Sources": "sources",
    "Stats": "stats",
    "Help": "help",
    "Research": "research",
    "Ask": "ask",
    "Summarize": "summarize",
    "Screenshot": "screenshot",
    "Diff": "diff",
    "Brand": "brand",
    "Prune": "prune",
}

# Overrides where the request struct name doesn't follow the `f"{Variant}Request"`
# convention. `Prune` uses `PruneMcpRequest` (not `PruneRequest`) because
# `axon_api::source::prune::PruneRequest` already owns that name for the
# service-layer DTO — the MCP wire type needed a distinct name to avoid
# ambiguity in glob-imported call sites.
STRUCT_NAME_OVERRIDES: dict[str, str] = {
    "Prune": "PruneMcpRequest",
}

STRUCT_TO_ACTION: dict[str, str] = {
    STRUCT_NAME_OVERRIDES.get(k, f"{k}Request"): v for k, v in VARIANT_TO_ACTION.items()
}

# Actions that remain on the shared AxonRequest enum (for REST/CLI
# compatibility) but are DENIED on the MCP surface: MCP_ACTION_SPECS omits them
# and crates/axon-mcp/src/server.rs rejects them with invalid_params before
# dispatch (issue #298 WS-G). They must not be documented as MCP tool actions.
MCP_DENIED_ACTIONS: frozenset[str] = frozenset({"sources", "domains", "stats"})

# Lifecycle families get special documentation treatment.
LIFECYCLE_FAMILIES: set[str] = {"extract"}

# Crawl-specific field descriptions (hardcoded -- not derivable from types).
CRAWL_FIELD_DESCRIPTIONS: dict[str, tuple[str, str]] = {
    # field_name: (default, description)
    "urls": ("--", "Seed URLs (required, non-empty array)"),
    "max_pages": ("0 (uncapped)", "Page limit"),
    "max_depth": ("10", "Max crawl depth"),
    "include_subdomains": ("false", "Include subdomains"),
    "respect_robots": ("false", "Honour robots.txt"),
    "discover_sitemaps": ("true", "Run sitemap backfill after crawl"),
    "sitemap_since_days": (
        "0",
        "Only backfill sitemap URLs with `<lastmod>` within last N days (0 = no filter)",
    ),
    "discover_llms_txt": (
        "true",
        "Probe `/llms.txt` at the site root and merge its links into the backfill candidate set and `map` discovery",
    ),
    "max_llms_txt_urls": (
        "512",
        "Max URLs taken from a single `/llms.txt` after scope filtering (0 = unlimited)",
    ),
    "render_mode": ("`auto_switch`", "`http`, `chrome`, `auto_switch`"),
    "delay_ms": ("0", "Per-request delay ms"),
}

# Runtime env vars -- not in schema.rs, hardcoded here.
# Each entry is either a string (bare var name) or a (name, description) tuple.
RUNTIME_ENV_VARS: list = [
    "QDRANT_URL",
    "TEI_URL",
    ("AXON_HEADLESS_GEMINI_CMD", "path to Gemini CLI (default: `gemini`)"),
    (
        "AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL",
        "preferred Gemini synthesis model override (optional)",
    ),
    (
        "AXON_HEADLESS_GEMINI_MODEL",
        "legacy alias for AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL",
    ),
    "TAVILY_API_KEY",
]

# Compatibility shims — still read at startup but only emit a warning; the
# feature they once controlled has been removed or replaced.
RUNTIME_ENV_VARS_DEPRECATED: list[str] = []

MCP_TRANSPORT_ENV_VARS: list[str] = [
    "AXON_HTTP_HOST",
    "AXON_HTTP_PORT",
]

MCP_AUTH_ENV_VARS: list[str] = [
    "AXON_HTTP_TOKEN",
    "AXON_AUTH_MODE",
    "AXON_PUBLIC_URL",
    "AXON_GOOGLE_CLIENT_ID",
    "AXON_GOOGLE_CLIENT_SECRET",
    "AXON_AUTH_ADMIN_EMAIL",
    "AXON_ALLOWED_REDIRECT_URIS",
]
