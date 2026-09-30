#!/usr/bin/env python3
"""Regression coverage for current-doc navigation and immutable-history audit."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from audit_docs import anchors, audit, category, link_targets, visible_lines


class DocsAuditTests(unittest.TestCase):
    def test_fences_and_frontmatter_are_not_navigation(self):
        text = "---\nlink: [x](missing)\n---\n~~~md\n[x](also-missing)\n~~~\n[good](target.md)\n"
        self.assertEqual(list(link_targets(text)), [(7, "target.md")])

    def test_inline_reference_and_nested_parentheses(self):
        text = "[a](folder/a(b).md#head) [b][Ref] [Ref]\n[ref]: <target.md#ok> \"Title\"\n"
        self.assertEqual([target for _, target in link_targets(text)], ["folder/a(b).md#head", "target.md#ok", "target.md#ok"])

    def test_duplicate_and_explicit_heading_anchors(self):
        text = "# Hello, `world`!\n# Hello, `world`!\nTitle\n=====\n<a id=\"custom\"></a>\n```md\n# Ignored\n```\n"
        result = anchors(text)
        self.assertTrue({"hello-world", "hello-world-1", "title", "custom"}.issubset(result))
        self.assertNotIn("ignored", result)

    def test_history_is_classified_not_rewritten_as_current(self):
        self.assertEqual(category("docs/reports/old.md", "old", False), "historical")
        self.assertEqual(category("docs/pipeline-unification/plans/old.md", "old", False), "design-history")
        self.assertEqual(category("docs/CLAUDE.md", "alias", True), "alias")
        self.assertEqual(category("docs/guides/current.md", "current", False), "current-prose")

    def test_inventory_reports_real_failures_without_writes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            contents = {
                "docs/guides/current.md": "# Current\n[ok](target.md#good)\n[bad](target.md#absent)\n[missing](gone.md)\n",
                "docs/guides/target.md": "# Good\n",
                "docs/reports/old.md": "[historical](does-not-exist.md)\n",
                "docs/reference/bad.json": "{broken}",
                "docs/reference/valid.jsonl": "{\"ok\": true}\n",
            }
            for name, text in contents.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            before = {name: (root / name).read_bytes() for name in contents}
            with patch("audit_docs.subprocess.check_output", side_effect=["\0".join(contents) + "\0", "a" * 40]):
                report = audit(root)
            kinds = {item["kind"] for item in report["findings"]}
            self.assertEqual(kinds, {"missing_anchor", "missing_link_target", "invalid_json"})
            self.assertEqual(report["summary"]["files"], len(contents))
            self.assertEqual(before, {name: (root / name).read_bytes() for name in contents})
            for item in report["files"]:
                self.assertEqual(item["sha256"], hashlib.sha256(before[item["path"]]).hexdigest())
            json.dumps(report)


if __name__ == "__main__":
    unittest.main()
