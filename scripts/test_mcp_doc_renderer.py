#!/usr/bin/env python3
"""Focused tests for MCP schema documentation rendering."""

from __future__ import annotations

import unittest
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

from mcp_doc_renderer import generate_markdown
from mcp_schema_models import EnumDef, FieldDef, StructDef
from mcp_schema_parser import parse_schema


class McpDocRendererTest(unittest.TestCase):
    def test_serde_defaults_are_optional_without_changing_their_types(self) -> None:
        structs, enums = parse_schema("""
            pub struct SourceRequest {
                pub source: Option<String>,
                #[serde(default)]
                pub limits: SourceLimits,
                #[serde(
                    alias = "adapter_options",
                    default = "AdapterOptions::default"
                )]
                pub options: AdapterOptions,
                pub required: String,
            }
        """)
        fields = structs["SourceRequest"].fields
        self.assertEqual([field.name for field in fields if field.is_optional],
                         ["source", "limits", "options"])
        self.assertEqual(fields[1].display_type, "SourceLimits")
        self.assertEqual(fields[2].aliases, ["adapter_options"])
        self.assertFalse(fields[3].has_default)
        markdown = generate_markdown(structs, enums)
        row = next(line for line in markdown.splitlines() if line.startswith("| `source` |"))
        columns = row.split("|")
        self.assertNotIn("`limits`", columns[2])
        self.assertNotIn("`options`", columns[2])
        self.assertIn("`limits`", columns[3])
        self.assertIn("`options`", columns[3])

    def test_rendered_contract_sections_include_current_defaults_and_resources(self) -> None:
        structs = {
            "AskRequest": StructDef(
                "AskRequest",
                [
                    FieldDef("query", "Option<String>"),
                    FieldDef("graph", "Option<bool>"),
                    FieldDef("diagnostics", "Option<bool>"),
                ],
            ),
            "CrawlRequest": StructDef(
                "CrawlRequest",
                [
                    FieldDef("subaction", "Option<CrawlSubaction>"),
                    FieldDef("urls", "Option<Vec<String>>"),
                    FieldDef("max_depth", "Option<usize>"),
                ],
            ),
        }
        enums = {
            "CrawlSubaction": EnumDef("CrawlSubaction", ["Start", "Status"]),
            "ResponseMode": EnumDef("ResponseMode", ["Path", "Inline"]),
        }

        markdown = generate_markdown(structs, enums)

        self.assertIn("| `max_depth` | usize | 10 | Max crawl depth |", markdown)
        self.assertIn("`graph` is a deprecated compatibility field", markdown)
        self.assertIn("- `AXON_AUTH_MODE`", markdown)
        self.assertIn("- `ui://axon/status-dashboard`", markdown)


if __name__ == "__main__":
    unittest.main()
