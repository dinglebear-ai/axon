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
VARIANT_TO_ACTION: dict[str, str] = {
    "Status": "status",
    "Crawl": "crawl",
    "Extract": "extract",
    "Embed": "embed",
    "Ingest": "ingest",
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
    "Artifacts": "artifacts",
    "Scrape": "scrape",
    "Research": "research",
    "Ask": "ask",
    "Summarize": "summarize",
    "Screenshot": "screenshot",
    "ElicitDemo": "elicit_demo",
}

STRUCT_TO_ACTION: dict[str, str] = {
    f"{k}Request": v for k, v in VARIANT_TO_ACTION.items()
}

# Lifecycle families get special documentation treatment.
LIFECYCLE_FAMILIES: set[str] = {"crawl", "extract", "embed", "ingest"}

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
    "render_mode": ("`auto_switch`", "`http`, `chrome`, `auto_switch`"),
    "delay_ms": ("0", "Per-request delay ms"),
}

# Runtime env vars -- not in schema.rs, hardcoded here.
# Each entry is either a string (bare var name) or a (name, description) tuple.
RUNTIME_ENV_VARS: list = [
    "QDRANT_URL",
    "TEI_URL",
    ("AXON_HEADLESS_GEMINI_CMD", "path to Gemini CLI (default: `gemini`)"),
    ("AXON_HEADLESS_GEMINI_MODEL", "Gemini model override (optional)"),
    "TAVILY_API_KEY",
]

# Compatibility shims — still read at startup but only emit a warning; the
# feature they once controlled has been removed or replaced.
RUNTIME_ENV_VARS_DEPRECATED: list[str] = []

MCP_TRANSPORT_ENV_VARS: list[str] = [
    "AXON_MCP_HTTP_HOST",
    "AXON_MCP_HTTP_PORT",
]

MCP_AUTH_ENV_VARS: list[str] = [
    "AXON_MCP_HTTP_TOKEN",
    "AXON_MCP_AUTH_MODE",
    "AXON_MCP_PUBLIC_URL",
    "AXON_MCP_GOOGLE_CLIENT_ID",
    "AXON_MCP_GOOGLE_CLIENT_SECRET",
    "AXON_MCP_AUTH_ADMIN_EMAIL",
    "AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS",
]
