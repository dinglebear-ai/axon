#!/usr/bin/env python3
"""Focused tests for MCP schema documentation rendering."""

from __future__ import annotations

import unittest
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

from mcp_doc_renderer import generate_markdown
from mcp_schema_models import EnumDef, FieldDef, StructDef


class McpDocRendererTest(unittest.TestCase):
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
