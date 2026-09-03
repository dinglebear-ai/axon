import importlib.util
import tempfile
import unittest
from unittest import mock
from pathlib import Path


SCRIPT = Path(__file__).with_name("qdrant-tune.py")
SPEC = importlib.util.spec_from_file_location("qdrant_tune", SCRIPT)
qdrant_tune = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(qdrant_tune)


class QdrantTuneTests(unittest.TestCase):
    def test_matrix_systematically_covers_all_six_optimizations(self):
        variants = qdrant_tune.default_variants()
        self.assertEqual(len({tuple(v[1:]) for v in variants}), len(variants))
        self.assertEqual([v.name for v in variants[:4]], ["rest-p1", "rest-p2", "rest-p3", "rest-p4"])
        self.assertTrue(any(v.transport == "grpc" for v in variants))
        self.assertTrue(any(v.async_writes for v in variants))
        self.assertTrue(any(v.bulk_load for v in variants))
        self.assertEqual(
            {(v.hnsw_m, v.hnsw_ef_construct) for v in variants if v.name.startswith("hnsw-")},
            {(32, 256), (16, 128), (16, 100)},
        )
        self.assertTrue(any(v.quantization for v in variants))
        self.assertTrue(any(not v.quantization for v in variants))
        controlled = {v.name: v for v in variants}
        self.assertEqual(controlled["rest-p2"].parallelism, controlled["grpc-p2"].parallelism)
        self.assertEqual(controlled["grpc-p2"].parallelism, controlled["grpc-async-p2"].parallelism)
        self.assertEqual(controlled["grpc-async-p2"].parallelism, controlled["grpc-async-bulk-p2"].parallelism)

    def test_variant_environment_maps_every_runtime_knob(self):
        variant = qdrant_tune.Variant("candidate", "grpc", 3, True, True, 16, 100, True)
        env = qdrant_tune.variant_environment(variant, "http://tootie:53334")
        self.assertEqual(env["AXON_QDRANT_TRANSPORT"], "grpc")
        self.assertEqual(env["QDRANT_GRPC_URL"], "http://tootie:53334")
        self.assertEqual(env["AXON_QDRANT_UPSERT_PARALLELISM"], "3")
        self.assertEqual(env["AXON_QDRANT_ASYNC_WRITES"], "true")
        self.assertEqual(env["AXON_QDRANT_BULK_LOAD"], "true")
        self.assertEqual(env["AXON_QDRANT_HNSW_M"], "16")
        self.assertEqual(env["AXON_QDRANT_HNSW_EF_CONSTRUCT"], "100")
        self.assertEqual(env["AXON_QDRANT_QUANTIZATION_ENABLED"], "true")

    def test_owned_collection_rejects_unscoped_names(self):
        self.assertEqual(
            qdrant_tune.owned_collection("run-1", "rest-p4"),
            "axon_qdrant_bench_run_1_rest_p4",
        )
        with self.assertRaisesRegex(ValueError, "owned benchmark prefix"):
            qdrant_tune.assert_owned_collection("axon")

    def test_recall_overlap_is_mean_top_k_overlap(self):
        baseline = [["a", "b", "c"], ["x", "y"]]
        candidate = [["a", "c", "z"], ["x", "q"]]
        self.assertAlmostEqual(qdrant_tune.mean_overlap(baseline, candidate), (2 / 3 + 1 / 2) / 2)

    def test_recall_overlap_counts_unique_result_keys(self):
        self.assertEqual(qdrant_tune.mean_overlap([["a", "a", "b"]], [["a", "a", "b"]]), 1.0)

    def test_result_key_prefers_stable_chunk_id(self):
        row = {"url": "https://example.test", "citation": {"chunk_id": "chunk-1"}}
        self.assertEqual(qdrant_tune.result_key(row), "chunk-1")

    def test_variant_filter_keeps_declared_order(self):
        selected = qdrant_tune.select_variants(qdrant_tune.default_variants(), ["rest-p2", "hnsw-16-100"])
        self.assertEqual([variant.name for variant in selected], ["rest-p2", "hnsw-16-100"])

    def test_frozen_corpus_accepts_an_existing_temporary_directory(self):
        with tempfile.TemporaryDirectory() as source_dir, tempfile.TemporaryDirectory() as destination_dir:
            source = Path(source_dir)
            destination = Path(destination_dir)
            (source / "code-claude-com-one.md").write_text("one")
            self.assertEqual(qdrant_tune.frozen_corpus(source, destination), 1)
            self.assertEqual((destination / "code-claude-com-one.md").read_text(), "one")

    def test_runs_are_repeated_and_order_is_interleaved(self):
        variants = qdrant_tune.default_variants()[:2]
        runs = qdrant_tune.interleaved_runs(variants, 3)
        self.assertEqual([v.name for _, v in runs], [variants[0].name, variants[1].name, variants[1].name, variants[0].name, variants[0].name, variants[1].name])
        with self.assertRaisesRegex(ValueError, "at least 2"):
            qdrant_tune.interleaved_runs(variants, 1)

    def test_summary_reports_median_and_spread(self):
        variant = qdrant_tune.default_variants()[0]._asdict()
        summary = qdrant_tune.summarize_rows([
            {"variant": variant, "seconds": 3.0},
            {"variant": variant, "seconds": 1.0},
            {"variant": variant, "seconds": 2.0},
        ])
        self.assertEqual(summary, [{"variant": variant["name"], "samples": 3, "median_seconds": 2.0, "min_seconds": 1.0, "max_seconds": 3.0}])

    def test_corpus_digest_changes_with_content(self):
        with tempfile.TemporaryDirectory() as directory:
            corpus = Path(directory)
            path = corpus / "code-claude-com-one.md"
            path.write_text("one")
            first = qdrant_tune.corpus_digest(corpus)
            path.write_text("two")
            self.assertNotEqual(first, qdrant_tune.corpus_digest(corpus))

    def test_equivalence_requires_complete_green_equal_point_runs(self):
        first, second = [variant._asdict() for variant in qdrant_tune.default_variants()[:2]]
        rows = [
            {"variant": first, "seconds": 1.0, "points": 10, "status": "green", "recall_overlap_at_10": 1.0},
            {"variant": second, "seconds": 2.0, "points": 10, "status": "green", "recall_overlap_at_10": 1.0},
        ]
        self.assertTrue(qdrant_tune.equivalence_report(rows, 1)["valid"])
        rows[1]["points"] = 9
        report = qdrant_tune.equivalence_report(rows, 1)
        self.assertFalse(report["valid"])
        self.assertIn("point counts differ or are missing", report["reasons"])

    def test_equivalence_rejects_divergent_retrieval(self):
        variant = qdrant_tune.default_variants()[0]._asdict()
        rows = [{
            "variant": variant,
            "seconds": 1.0,
            "points": 10,
            "status": "green",
            "recall_overlap_at_10": 0.8,
        }]
        report = qdrant_tune.equivalence_report(rows, 1)
        self.assertFalse(report["valid"])
        self.assertIn("retrieval overlap is below 1.0000", report["reasons"])

    def test_main_returns_nonzero_for_invalid_equivalence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "axon"
            binary.write_bytes(b"binary")
            source = root / "source"
            source.mkdir()
            (source / "code-claude-com-one.md").write_text("one")
            argv = [
                "qdrant-tune.py", "--execute", "--binary", str(binary),
                "--source", str(source), "--variant", "rest-p1", "--repetitions", "2",
            ]
            with mock.patch("sys.argv", argv), \
                 mock.patch.object(qdrant_tune, "run_variant", side_effect=RuntimeError("failed")), \
                 mock.patch.object(qdrant_tune, "delete_owned_collection"), \
                 mock.patch.object(qdrant_tune, "service_identity", return_value={}):
                self.assertEqual(qdrant_tune.main(), 2)


if __name__ == "__main__":
    unittest.main()
