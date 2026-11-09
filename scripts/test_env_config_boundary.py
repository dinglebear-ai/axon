#!/usr/bin/env python3
"""Regression coverage for authored-source environment inventory boundaries."""
from __future__ import annotations
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

CHECKER = Path(__file__).with_name("check-env-config-boundary.py")
SPEC = importlib.util.spec_from_file_location("env_config_boundary", CHECKER)
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


class SourceBoundaryTests(unittest.TestCase):
    def test_installed_dependencies_are_excluded_without_hiding_authored_tooling(self):
        # Construct synthetic keys at runtime so scanning this test does not
        # introduce fixture names into the repository's runtime env contract.
        dependency_key = "OPEN" + "AI_" + "DEPENDENCY_BOUNDARY_FIXTURE"
        authored_key = "AX" + "ON_" + "AUTHORED_BOUNDARY_FIXTURE"
        similar_name_key = "AX" + "ON_" + "SIMILAR_PATH_BOUNDARY_FIXTURE"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tooling = root / "scripts/e2e/tooling"
            dependency = tooling / "node_modules/example/dist/config.js"
            dependency.parent.mkdir(parents=True)
            dependency.write_text(dependency_key)
            authored = tooling / "helper.js"
            authored.write_text(authored_key)
            similar_name = tooling / "node_modules_helper.js"
            similar_name.write_text(similar_name_key)
            with patch.object(checker, "ROOT", root):
                found = checker.scan_env_tokens()
            self.assertNotIn(dependency_key, found)
            self.assertEqual({"scripts/e2e/tooling/helper.js"}, found[authored_key])
            self.assertEqual({"scripts/e2e/tooling/node_modules_helper.js"}, found[similar_name_key])


if __name__ == "__main__":
    unittest.main()
