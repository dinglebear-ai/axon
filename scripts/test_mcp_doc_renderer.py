#!/usr/bin/env python3
"""Regression tests for schema-driven docs, provenance, and read-only checking."""
from __future__ import annotations
import copy
import hashlib
import html
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from doc_schema import ContractError, cell, load_contract, resolve_schema
from generate_action_docs import BEGIN, END, Surface, generated_block, insert_block, read_surfaces, generate_index
from mcp_doc_renderer import generate_markdown

ROOT = Path(__file__).resolve().parents[1]


def mcp_contract():
    return {
        "$defs": {
            "Input": {"type": "object", "additionalProperties": False, "required": ["query"], "properties": {
                "query": {"type": "string", "description": "one | two"},
                "limit": {"type": "integer", "default": 10, "minimum": 1},
                "optional": {"type": ["string", "null"]},
                "kind": {"$ref": "#/$defs/Kind"}}},
            "Kind": {"type": "string", "enum": ["one", "two"]},
            "AxonToolResponse": {"type": "object", "properties": {"request_id": {"type": "string"}}, "required": ["request_id"]},
        },
        "x-axon": {"contract_version": "test", "projection_default": "legacy", "projection_values": ["legacy", "atomic", "both"],
            "operation_count": 2, "live_action_count": 2, "operations": [
                {"name": "source", "action": "source", "subaction": None, "description": "Acquire sources", "required_scope": "axon:write", "read_only": False, "task_support": "forbidden", "inputSchema": {"$ref": "#/$defs/Input"}},
                {"name": "artifacts_content", "action": "artifacts", "subaction": "content", "description": "Read an opaque artifact", "required_scope": "axon:read", "read_only": True, "task_support": "optional", "inputSchema": {"$ref": "#/$defs/Input"}},
            ]},
    }


def write_contract(root, path, producer, document):
    source = root / "source.rs"
    source.write_text("pub struct Current;\n")
    document = copy.deepcopy(document)
    document.setdefault("x-axon", {}).update({"generated_by": producer, "source_inputs": [{"path": "source.rs", "kind": "rust_module", "checksum": "sha256:" + hashlib.sha256(source.read_bytes()).hexdigest()}]})
    target = root / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(document))


def seed(root):
    write_contract(root, "docs/reference/mcp/tool-schema.json", "cargo xtask schemas mcp", mcp_contract())
    write_contract(root, "docs/reference/cli/commands.json", "cargo xtask schemas cli", {"commands": [{"name": "crawl", "path": ["crawl"]}, {"name": "source", "path": ["source"]}]})
    write_contract(root, "docs/reference/rest/openapi.json", "cargo xtask schemas openapi", {"routes": [{"path": "/v1/crawl", "method": "POST"}, {"path": "/v1/sources", "method": "POST"}]})
    directory = root / "docs/reference/actions"
    directory.mkdir(parents=True)
    (directory / "crawl.md").write_text("# Crawl\n\nHandwritten facts.\n")


class McpDocRendererTest(unittest.TestCase):
    def test_fields_come_from_closed_schema_not_rust_name_guesses(self):
        result = generate_markdown(mcp_contract())
        self.assertIn("artifacts_content", result)
        self.assertIn("| query | yes |", result)
        self.assertIn("| limit | no |", result)
        self.assertIn('"default":10', result)
        self.assertIn('"enum":["one","two"]', html.unescape(result))
        self.assertIn("&#91;", result)
        self.assertNotIn('"enum":[', result)
        self.assertIn("one &#124; two", result)
        self.assertIn("| request_id | yes |", result)
        self.assertIn("## Tool Projections", result)
        self.assertIn("## Task-Augmented Calls", result)
        self.assertIn("opaque artifact_id", result)
        self.assertNotIn("Last Modified:", result)

    def test_every_operation_has_a_field_section_and_generation_is_stable(self):
        doc = mcp_contract()
        first = generate_markdown(doc)
        doc["x-axon"]["operations"].reverse()
        # Catalog order does not change field order; provenance hash captures raw contract order.
        self.assertEqual(first.split("## Contract", 1)[1], generate_markdown(doc).split("## Contract", 1)[1])
        self.assertEqual(generate_markdown(doc), generate_markdown(doc))
        self.assertEqual(first.count("### "), 2)

    def test_null_operation_scope_preserves_transport_auth_distinction(self):
        doc = mcp_contract()
        doc["x-axon"]["operations"][0]["required_scope"] = None
        self.assertIn("source", generate_markdown(doc))

    def test_missing_reference_fails_instead_of_empty_schema(self):
        doc = mcp_contract()
        doc["x-axon"]["operations"][0]["inputSchema"] = {"$ref": "#/$defs/Missing"}
        with self.assertRaisesRegex(ContractError, "missing schema reference"):
            generate_markdown(doc)

    def test_duplicate_catalog_and_stale_counts_fail(self):
        for mutation in ("duplicate", "count"):
            doc = mcp_contract()
            if mutation == "duplicate":
                doc["x-axon"]["operations"][1]["name"] = "source"
            else:
                doc["x-axon"]["operation_count"] = 999
            with self.assertRaises(ContractError):
                generate_markdown(doc)

    def test_cyclic_reference_fails(self):
        with self.assertRaisesRegex(ContractError, "cyclic"):
            resolve_schema({"$defs": {"A": {"$ref": "#/$defs/A"}}}, {"$ref": "#/$defs/A"})

    def test_markdown_cell_keeps_one_column(self):
        value = cell("a|b\nc<d" + chr(96))
        self.assertNotIn("|", value)
        self.assertNotIn("\n", value)
        self.assertIn("&lt;", value)


class ActionDocRendererTest(unittest.TestCase):
    def test_restored_commands_and_system_tools_come_from_json(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            seed(root)
            surfaces = read_surfaces(root)
            self.assertEqual(surfaces["crawl"].cli, ("axon crawl",))
            self.assertEqual(surfaces["crawl"].rest, ("POST /v1/crawl",))
            self.assertEqual(surfaces["artifacts"].mcp, ("artifacts_content",))
            index = generate_index(root / "docs/reference/actions", surfaces)
            self.assertIn("artifacts_content", index)
            self.assertNotIn("src/cli/commands.rs", index)
            self.assertNotIn("Not inventoried", index)
            self.assertNotIn("Removed;", index)

    def test_transcript_ingestion_does_not_claim_mobile_chat_routes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            seed(root)
            write_contract(root, "docs/reference/cli/commands.json", "cargo xtask schemas cli", {"commands": [{"path": ["sessions"]}, {"path": ["source"]}]})
            write_contract(root, "docs/reference/rest/openapi.json", "cargo xtask schemas openapi", {"routes": [{"path": "/v1/mobile/sessions", "method": "GET"}, {"path": "/v1/sources", "method": "POST"}]})
            surfaces = read_surfaces(root)
            self.assertEqual(surfaces["sessions"].cli, ("axon sessions",))
            self.assertEqual(surfaces["sessions"].mcp, ("source",))
            self.assertEqual(surfaces["sessions"].rest, ("POST /v1/sources",))
            self.assertEqual(surfaces["mobile"].rest, ("GET /v1/mobile/sessions",))

    def test_marked_update_preserves_all_handwritten_bytes(self):
        text = "---\ntitle: Keep\n---\n# Title\n\n" + BEGIN + "\nold\n" + END + "\n\nUnrelated prose.\n"
        result = insert_block(text, BEGIN + "\nnew\n" + END)
        self.assertEqual(result, text.replace("\nold\n", "\nnew\n"))

    def test_unmarked_page_preserves_frontmatter(self):
        original = "---\ntitle: Keep\n---\n# Title\nLast Modified: original\n\nBody\n"
        result = insert_block(original, BEGIN + "\nnew\n" + END)
        self.assertTrue(result.startswith("---\ntitle: Keep\n---\n# Title\nLast Modified: original\n"))
        self.assertTrue(result.endswith("\nBody\n"))

    def test_malformed_markers_fail(self):
        for text in (BEGIN, END, END + BEGIN, BEGIN + END + BEGIN + END):
            with self.assertRaisesRegex(ContractError, "markers"):
                insert_block(text, "new")

    def test_generated_blocks_have_no_trailing_whitespace(self):
        for notes in ("", "Documented caveat.", "Documented caveat.   "):
            with self.subTest(notes=notes):
                result = generated_block(Surface("example", (), (), (), notes))
                self.assertTrue(all(line == line.rstrip() for line in result.splitlines()))

    def test_unknown_page_does_not_invent_a_cli_command(self):
        result = generated_block(Surface("invented", (), (), ()))
        self.assertNotIn("axon invented", result)
        self.assertIn("Not exposed", result)


class GeneratorCliTest(unittest.TestCase):
    def test_check_is_read_only_and_refresh_is_idempotent_for_both_renderers(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            seed(root)
            for name in ("generate_action_docs.py", "generate_mcp_schema_doc.py"):
                command = [sys.executable, str(ROOT / "scripts" / name), "--repo-root", str(root)]
                before = {p.relative_to(root): p.read_bytes() for p in root.rglob("*") if p.is_file()}
                self.assertEqual(subprocess.run(command + ["--check"], capture_output=True).returncode, 1)
                self.assertEqual(before, {p.relative_to(root): p.read_bytes() for p in root.rglob("*") if p.is_file()})
                subprocess.run(command, check=True, capture_output=True)
                first = {p.relative_to(root): p.read_bytes() for p in root.rglob("*") if p.is_file()}
                subprocess.run(command, check=True, capture_output=True)
                self.assertEqual(first, {p.relative_to(root): p.read_bytes() for p in root.rglob("*") if p.is_file()})
                self.assertEqual(subprocess.run(command + ["--check"], capture_output=True).returncode, 0)

    def test_stale_upstream_provenance_is_not_a_successful_empty_catalog(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            seed(root)
            (root / "source.rs").write_text("changed\n")
            for name in ("generate_action_docs.py", "generate_mcp_schema_doc.py"):
                result = subprocess.run([sys.executable, str(ROOT / "scripts" / name), "--repo-root", str(root), "--check"], capture_output=True, text=True)
                self.assertEqual(result.returncode, 2)
                self.assertIn("stale source input", result.stderr)

    def test_source_path_cannot_escape_repository(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            seed(root)
            path = root / "docs/reference/cli/commands.json"
            doc = json.loads(path.read_text())
            doc["x-axon"]["source_inputs"][0]["path"] = "../escape.rs"
            path.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ContractError, "stay inside"):
                load_contract(root, str(path.relative_to(root)), "cargo xtask schemas cli")


if __name__ == "__main__":
    unittest.main()
