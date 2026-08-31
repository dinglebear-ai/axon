#!/usr/bin/env python3
from __future__ import annotations

import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

CORPUS_DIR = Path(__file__).resolve().parent / "corpus"
SPEC = importlib.util.spec_from_file_location("corpus_validate", CORPUS_DIR / "validate.py")
assert SPEC and SPEC.loader
validator = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(validator)


class CorpusContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.manifest = json.loads((CORPUS_DIR / "manifest.json").read_text(encoding="utf-8"))

    def write_allowlist(self, directory: str) -> None:
        (Path(directory) / "license-allowlist.json").write_bytes(
            (CORPUS_DIR / "license-allowlist.json").read_bytes()
        )

    def test_checked_in_corpus_is_valid_and_reportable(self) -> None:
        report = validator.validate()
        self.assertEqual("1.0.0", report["corpus_version"])
        self.assertRegex(report["corpus_checksum"], r"^[0-9a-f]{64}$")
        self.assertEqual("valid", report["status"])

    def test_semantic_cases_are_explicit(self) -> None:
        expected = json.loads((CORPUS_DIR / "v1/expected/semantics.json").read_text())
        self.assertTrue(expected["facts"])
        self.assertTrue(expected["citations"])
        self.assertTrue(expected["distractors"])
        self.assertTrue(expected["contradictions"])
        self.assertTrue(expected["graph"]["edges"])
        self.assertEqual({"superseded", "current"}, {m["state"] for m in expected["memory"]})

    def test_stress_expansion_is_deterministic_and_bounded(self) -> None:
        recipe = json.loads((CORPUS_DIR / "v1/stress/recipe.json").read_text())
        first = validator.stress_record(recipe, 42)
        self.assertEqual(first, validator.stress_record(recipe, 42))
        self.assertNotEqual(first, validator.stress_record(recipe, 43))
        self.assertIn("group-42", first)
        self.assertEqual("explicit-capacity-only", self.manifest["tiers"]["stress"]["selection"])

    def test_oversized_rejection_fixture_is_explicit_and_independent(self) -> None:
        oversized = next(
            document for document in self.manifest["documents"]
            if document.get("expected_parse") == "reject_oversized"
        )
        fixture = CORPUS_DIR / oversized["path"]
        self.assertGreater(fixture.stat().st_size, oversized["declared_input_limit_bytes"])
        self.assertGreaterEqual(fixture.stat().st_size, oversized["minimum_fixture_bytes"])
        self.assertNotIn("boundary", fixture.name)

    def test_validator_rejects_unversioned_byte_change(self) -> None:
        mutated = copy.deepcopy(self.manifest)
        mutated["documents"][0]["sha256"] = "0" * 64
        with tempfile.TemporaryDirectory() as directory:
            self.write_allowlist(directory)
            temporary = Path(directory) / "manifest.json"
            temporary.write_text(json.dumps(mutated), encoding="utf-8")
            # Point paths back at the real corpus while retaining a mutable manifest.
            for record in validator.manifest_records(mutated):
                record["path"] = str((CORPUS_DIR / record["path"]).resolve())
            temporary.write_text(json.dumps(mutated), encoding="utf-8")
            with self.assertRaisesRegex(validator.CorpusError, "checksum mismatch"):
                validator.validate(temporary)

    def test_validator_rejects_duplicate_ids_and_broken_lineage(self) -> None:
        mutated = copy.deepcopy(self.manifest)
        mutated["documents"][1]["id"] = mutated["documents"][0]["id"]
        mutated["revisions"][1]["predecessor"] = "revision.missing"
        with tempfile.TemporaryDirectory() as directory:
            self.write_allowlist(directory)
            temporary = Path(directory) / "manifest.json"
            for record in validator.manifest_records(mutated):
                record["path"] = str((CORPUS_DIR / record["path"]).resolve())
            temporary.write_text(json.dumps(mutated), encoding="utf-8")
            with self.assertRaises(validator.CorpusError) as raised:
                validator.validate(temporary)
            self.assertIn("IDs must be unique", str(raised.exception))
            self.assertIn("broken lineage", str(raised.exception))

    def test_validator_rejects_missing_fixture(self) -> None:
        mutated = copy.deepcopy(self.manifest)
        mutated["documents"][0]["path"] = "v1/documents/micro/does-not-exist.md"
        with tempfile.TemporaryDirectory() as directory:
            self.write_allowlist(directory)
            temporary = Path(directory) / "manifest.json"
            for record in validator.manifest_records(mutated)[1:]:
                record["path"] = str((CORPUS_DIR / record["path"]).resolve())
            temporary.write_text(json.dumps(mutated), encoding="utf-8")
            with self.assertRaisesRegex(validator.CorpusError, "missing corpus files"):
                validator.validate(temporary)

    def test_rewriter_requires_relevant_component_version_bump(self) -> None:
        mutated = copy.deepcopy(self.manifest)
        mutated["documents"][0]["sha256"] = "0" * 64
        with tempfile.TemporaryDirectory() as directory:
            self.write_allowlist(directory)
            temporary = Path(directory) / "manifest.json"
            for record in validator.manifest_records(mutated):
                record["path"] = str((CORPUS_DIR / record["path"]).resolve())
            temporary.write_text(json.dumps(mutated), encoding="utf-8")
            baseline = json.loads((CORPUS_DIR / "release-baseline.json").read_text())
            (Path(directory) / "release-baseline.json").write_text(json.dumps(baseline))
            with self.assertRaisesRegex(validator.CorpusError, "requires component version bump: bytes"):
                validator.rewrite_checksums(temporary)

    def test_license_and_secret_scanners_reject_unsafe_corpus(self) -> None:
        unsafe_license = copy.deepcopy(self.manifest)
        unsafe_license["license_spdx"] = "LicenseRef-Proprietary"
        with tempfile.TemporaryDirectory() as directory:
            self.write_allowlist(directory)
            temporary = Path(directory) / "manifest.json"
            for record in validator.manifest_records(unsafe_license):
                record["path"] = str((CORPUS_DIR / record["path"]).resolve())
            temporary.write_text(json.dumps(unsafe_license), encoding="utf-8")
            with self.assertRaisesRegex(validator.CorpusError, "not allowlisted"):
                validator.validate(temporary)

        unsafe_secret = copy.deepcopy(self.manifest)
        with tempfile.TemporaryDirectory() as directory:
            self.write_allowlist(directory)
            secret = Path(directory) / "secret.txt"
            # Deliberately credential-shaped inert fixture proving the corpus
            # validator rejects it; it never leaves this temporary directory.
            # lgtm [py/clear-text-storage-sensitive-data]
            secret.write_text("Authorization: Bearer abcdefghijklmnopqrstuvwxyz.123456")
            unsafe_secret["documents"][0]["path"] = str(secret)
            unsafe_secret["documents"][0]["sha256"] = validator.sha256(secret)
            unsafe_secret["corpus_checksum"] = validator.corpus_checksum(unsafe_secret)
            temporary = Path(directory) / "manifest.json"
            for record in validator.manifest_records(unsafe_secret)[1:]:
                record["path"] = str((CORPUS_DIR / record["path"]).resolve())
            temporary.write_text(json.dumps(unsafe_secret), encoding="utf-8")
            with self.assertRaisesRegex(validator.CorpusError, "credential-like data"):
                validator.validate(temporary)

    def test_revision_lineage_preserves_identity_and_change_meaning(self) -> None:
        revisions = self.manifest["revisions"]
        self.assertEqual(1, len({revision["source_id"] for revision in revisions}))
        unchanged = next(r for r in revisions if r["change"] == "unchanged")
        changed = next(r for r in revisions if r["change"] == "changed")
        prior = {r["id"]: r for r in revisions}
        self.assertEqual(prior[unchanged["predecessor"]]["sha256"], unchanged["sha256"])
        self.assertNotEqual(prior[changed["predecessor"]]["sha256"], changed["sha256"])


if __name__ == "__main__":
    unittest.main()
