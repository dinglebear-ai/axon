#!/usr/bin/env python3
"""Regression tests for complete CI timing API inventories."""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).parents[1] / "scripts" / "ci" / "report_workflow_timings.py"
SPEC = importlib.util.spec_from_file_location("report_workflow_timings", SCRIPT)
assert SPEC and SPEC.loader
timings = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(timings)


class PaginationTests(unittest.TestCase):
    def test_repository_workflows_reads_records_after_first_page(self) -> None:
        first = [
            {
                "id": index,
                "name": f"workflow-{index:03}",
                "path": f".github/workflows/{index:03}.yml",
                "state": "active",
            }
            for index in range(100)
        ]
        final = [
            {
                "id": 100,
                "name": "workflow-100",
                "path": ".github/workflows/100.yml",
                "state": "active",
            }
        ]
        with patch.object(
            timings,
            "gh_api",
            side_effect=[{"workflows": first}, {"workflows": final}],
        ) as api:
            workflows = timings.repository_workflows("dinglebear-ai/axon")

        self.assertEqual(len(workflows), 101)
        self.assertEqual(workflows[-1]["id"], 100)
        self.assertEqual(api.call_args_list[0].args[1]["page"], "1")
        self.assertEqual(api.call_args_list[1].args[1]["page"], "2")

    def test_run_record_includes_jobs_after_first_page(self) -> None:
        def job(index: int, conclusion: str = "success") -> dict[str, object]:
            return {
                "name": f"job-{index:03}",
                "conclusion": conclusion,
                "started_at": "2026-08-08T00:00:00Z",
                "completed_at": "2026-08-08T00:00:01Z",
                "runner_name": "runner",
            }

        first = [job(index) for index in range(100)]
        final = [job(100), job(101, "skipped")]
        run = {
            "id": 42,
            "workflow_id": 7,
            "path": ".github/workflows/ci.yml",
            "name": "CI",
            "event": "push",
            "conclusion": "success",
            "head_sha": "abc",
            "html_url": "https://example.invalid/run/42",
            "created_at": "2026-08-08T00:00:00Z",
            "updated_at": "2026-08-08T00:00:02Z",
        }
        with patch.object(
            timings,
            "gh_api",
            side_effect=[{"jobs": first}, {"jobs": final}],
        ):
            record = timings.run_record("dinglebear-ai/axon", "candidate", run)

        self.assertEqual(len(record["jobs"]), 102)
        self.assertEqual(record["executed_jobs"], 101)
        self.assertEqual(record["skipped_jobs"], 1)
        self.assertEqual(record["runner_seconds"], 101.0)

    def test_sha_inventory_reads_workflow_runs_after_first_page(self) -> None:
        first = [{"id": index, "workflow_id": index} for index in range(100)]
        final = [{"id": 100, "workflow_id": 100}]
        with patch.object(
            timings,
            "gh_api",
            side_effect=[{"workflow_runs": first}, {"workflow_runs": final}],
        ):
            runs = timings.runs_for_sha("dinglebear-ai/axon", "abc", {100})

        self.assertEqual(runs, final)


if __name__ == "__main__":
    unittest.main()
