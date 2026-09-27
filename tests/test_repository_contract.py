"""Regression tests for Axon's narrow adapter over the immutable fleet contract."""
import importlib.util
import os
from dataclasses import dataclass
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("axon_repository_contract", ROOT / "scripts/check_repository_contract.py")
contract = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = contract
SPEC.loader.exec_module(contract)


@dataclass(frozen=True)
class Finding:
    check: str
    path: str
    message: str


@unittest.skipUnless(os.name == "posix", "fixture requires native Git symlinks")
class RepositoryContractTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name)
        self.git("init", "-q")
        self.valid_scope(self.repo)
        self.git("add", ".")

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.repo), *args], text=True)

    def valid_scope(self, directory):
        directory.mkdir(parents=True, exist_ok=True)
        (directory / "AGENTS.md").write_text("# Shared guidance\n")
        for name in ("CLAUDE.md", "GEMINI.md"):
            (directory / name).symlink_to("AGENTS.md")

    def findings(self):
        return contract.check_agent_symlinks(self.repo, Finding)

    def test_accepts_canonical_layout(self):
        self.assertEqual(self.findings(), [])

    def test_rejects_old_reverse_layout(self):
        (self.repo / "CLAUDE.md").unlink()
        (self.repo / "GEMINI.md").unlink()
        (self.repo / "AGENTS.md").rename(self.repo / "CLAUDE.md")
        (self.repo / "AGENTS.md").symlink_to("CLAUDE.md")
        (self.repo / "GEMINI.md").symlink_to("CLAUDE.md")
        self.git("add", ".")
        self.assertTrue(self.findings())

    def test_rejects_copied_alias(self):
        alias = self.repo / "CLAUDE.md"
        alias.unlink()
        alias.write_text("AGENTS.md")
        self.git("add", ".")
        self.assertTrue(self.findings())

    def test_rejects_missing_alias(self):
        self.git("rm", "--cached", "GEMINI.md")
        self.assertTrue(self.findings())

    def test_requires_root_guidance(self):
        self.git("rm", "--cached", "AGENTS.md", "CLAUDE.md", "GEMINI.md")
        self.assertTrue(self.findings())

    def test_accepts_nested_scope(self):
        self.valid_scope(self.repo / "nested")
        self.git("add", ".")
        self.assertEqual(self.findings(), [])

    def test_rejects_nested_orphan_alias(self):
        (self.repo / "nested").mkdir()
        (self.repo / "nested/CLAUDE.md").symlink_to("AGENTS.md")
        self.git("add", ".")
        self.assertTrue(self.findings())

    def test_rejects_absolute_and_chained_targets(self):
        alias = self.repo / "GEMINI.md"
        for target in (str(self.repo / "AGENTS.md"), "CLAUDE.md"):
            with self.subTest(target=target):
                alias.unlink()
                alias.symlink_to(target)
                self.git("add", ".")
                self.assertTrue(self.findings())

    def test_uses_index_not_unstaged_filesystem(self):
        alias = self.repo / "CLAUDE.md"
        alias.unlink()
        alias.write_text("not a symlink")
        self.assertEqual(self.findings(), [])
        self.git("add", ".")
        alias.unlink()
        alias.symlink_to("AGENTS.md")
        self.assertTrue(self.findings())

    def test_keeps_unrelated_fleet_findings_and_restores_original_hook(self):
        old_hook = lambda repo: [Finding("legacy", "", "old direction")]
        seen = []
        fleet = SimpleNamespace(Finding=Finding, check_symlinks=old_hook)
        def upstream_check(repo, profile):
            seen.append(profile)
            return fleet.check_symlinks(repo) + [Finding("other-check", "file", "must fail")]
        fleet.check = upstream_check
        findings = contract.check(self.repo, "rust", fleet)
        self.assertEqual([item.check for item in findings], ["other-check"])
        self.assertEqual(seen, ["rust"])
        self.assertIs(fleet.check_symlinks, old_hook)

    def test_preserves_upstream_exceptions(self):
        old_hook = lambda repo: []
        def fail(repo, profile):
            raise RuntimeError("upstream failure")
        fleet = SimpleNamespace(Finding=Finding, check_symlinks=old_hook, check=fail)
        with self.assertRaisesRegex(RuntimeError, "upstream failure"):
            contract.check(self.repo, "rust", fleet)
        self.assertIs(fleet.check_symlinks, old_hook)

    def test_rejects_unsupported_upstream_interface(self):
        source = self.repo / "unsupported.py"
        source.write_text("def check(*args): return []\n")
        with self.assertRaisesRegex(RuntimeError, "unsupported fleet implementation"):
            contract.load_fleet(source)


if __name__ == "__main__":
    unittest.main()
