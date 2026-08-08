#!/usr/bin/env python3
"""Render GitHub Actions run/job timing data as Markdown and JSON."""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
from datetime import datetime
from pathlib import Path
from typing import Any


def gh_api(path: str, fields: dict[str, str] | None = None) -> Any:
    command = ["gh", "api", "-X", "GET", path]
    for key, value in (fields or {}).items():
        command.extend(["-f", f"{key}={value}"])
    return json.loads(subprocess.check_output(command, text=True))


def instant(value: str | None) -> datetime | None:
    if not value:
        return None
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def seconds_between(start: str | None, end: str | None) -> float:
    start_at = instant(start)
    end_at = instant(end)
    if start_at is None or end_at is None or end_at < start_at:
        return 0.0
    return (end_at - start_at).total_seconds()


def duration(seconds: float) -> str:
    total = max(0, round(seconds))
    hours, remainder = divmod(total, 3600)
    minutes, secs = divmod(remainder, 60)
    if hours:
        return f"{hours}h {minutes:02d}m {secs:02d}s"
    if minutes:
        return f"{minutes}m {secs:02d}s"
    return f"{secs}s"


def run_record(repo: str, label: str, run: dict[str, Any]) -> dict[str, Any]:
    jobs = gh_api(
        f"repos/{repo}/actions/runs/{run['id']}/jobs",
        {"filter": "latest", "per_page": "100"},
    )["jobs"]
    executed = [job for job in jobs if job.get("conclusion") != "skipped"]
    job_seconds = {
        job["name"]: seconds_between(job.get("started_at"), job.get("completed_at"))
        for job in executed
    }
    longest_name, longest_seconds = max(job_seconds.items(), key=lambda item: item[1], default=("-", 0.0))
    return {
        "label": label,
        "workflow": run["name"],
        "run_id": run["id"],
        "event": run["event"],
        "conclusion": run.get("conclusion") or run.get("status"),
        "head_sha": run["head_sha"],
        "url": run["html_url"],
        "wall_seconds": seconds_between(run.get("created_at"), run.get("updated_at")),
        "runner_seconds": sum(job_seconds.values()),
        "executed_jobs": len(executed),
        "skipped_jobs": len(jobs) - len(executed),
        "longest_job": longest_name,
        "longest_job_seconds": longest_seconds,
        "jobs": [
            {
                "name": job["name"],
                "conclusion": job.get("conclusion"),
                "seconds": seconds_between(job.get("started_at"), job.get("completed_at")),
                "runner": job.get("runner_name") or "",
            }
            for job in jobs
        ],
    }


def runs_for_sha(repo: str, sha: str) -> list[dict[str, Any]]:
    payload = gh_api(
        f"repos/{repo}/actions/runs",
        {"head_sha": sha, "status": "completed", "per_page": "100"},
    )
    return payload["workflow_runs"]


def recent_runs(repo: str, branch: str, limit: int) -> list[dict[str, Any]]:
    payload = gh_api(
        f"repos/{repo}/actions/runs",
        {"branch": branch, "status": "completed", "per_page": "100"},
    )
    selected: list[dict[str, Any]] = []
    counts: dict[str, int] = {}
    for run in payload["workflow_runs"]:
        name = run["name"]
        if counts.get(name, 0) >= limit:
            continue
        counts[name] = counts.get(name, 0) + 1
        selected.append(run)
    return selected


def percentile(values: list[float], fraction: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, round((len(ordered) - 1) * fraction)))
    return ordered[index]


def render(records: list[dict[str, Any]], mode: str) -> str:
    lines = ["# Axon CI timing report", ""]
    if mode == "sha":
        lines.extend(
            [
                "| Snapshot | Workflow | Result | Wall | Runner time | Jobs run/skipped | Longest job |",
                "|---|---|---:|---:|---:|---:|---|",
            ]
        )
        for record in sorted(records, key=lambda item: (item["label"], item["workflow"])):
            lines.append(
                f"| {record['label']} | [{record['workflow']}]({record['url']}) | {record['conclusion']} | "
                f"{duration(record['wall_seconds'])} | {duration(record['runner_seconds'])} | "
                f"{record['executed_jobs']}/{record['skipped_jobs']} | "
                f"{record['longest_job']} ({duration(record['longest_job_seconds'])}) |"
            )
    else:
        grouped: dict[str, list[dict[str, Any]]] = {}
        for record in records:
            grouped.setdefault(record["workflow"], []).append(record)
        lines.extend(
            [
                "| Workflow | Samples | Median wall | P95 wall | Median runner time |",
                "|---|---:|---:|---:|---:|",
            ]
        )
        for workflow, samples in sorted(grouped.items()):
            walls = [sample["wall_seconds"] for sample in samples]
            runners = [sample["runner_seconds"] for sample in samples]
            lines.append(
                f"| {workflow} | {len(samples)} | {duration(statistics.median(walls))} | "
                f"{duration(percentile(walls, 0.95))} | {duration(statistics.median(runners))} |"
            )

    lines.extend(["", "Runner time is the sum of non-skipped job durations; wall time is end-to-end run duration.", ""])
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", required=True)
    parser.add_argument("--sha", action="append", default=[], metavar="LABEL=SHA")
    parser.add_argument("--branch", default="main")
    parser.add_argument("--recent", type=int, default=5)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--json-output", type=Path)
    args = parser.parse_args()

    records: list[dict[str, Any]] = []
    mode = "sha" if args.sha else "recent"
    if args.sha:
        for spec in args.sha:
            label, separator, sha = spec.partition("=")
            if not separator or not label or not sha:
                parser.error(f"invalid --sha value {spec!r}; expected LABEL=SHA")
            for run in runs_for_sha(args.repo, sha):
                records.append(run_record(args.repo, label, run))
    else:
        for run in recent_runs(args.repo, args.branch, args.recent):
            records.append(run_record(args.repo, "recent", run))

    args.output.write_text(render(records, mode))
    if args.json_output:
        args.json_output.write_text(json.dumps(records, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
