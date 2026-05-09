#!/usr/bin/env python3
"""
Reingest all seeds from the latest live-export JSON.

Reads rebuild_seeds from the export and fires:
  - crawl jobs for all seed URLs
  - ingest jobs for all GitHub repos (with include_source flags)
  - ingest jobs for YouTube targets

Usage: python3 scripts/reingest.py [--dry-run] [--only crawl|github|youtube]
"""

import json
import os
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).parent.parent
BINARY = REPO / "target" / "release" / "axon"
EXPORT = (
    REPO / ".cache/axon-rust/output/live-export-20260320-v3-rebuild-after-scrape2.json"
)

# ── CLI flags ──────────────────────────────────────────────────────────────────
dry_run = "--dry-run" in sys.argv
only = None
for i, arg in enumerate(sys.argv[1:]):
    if arg == "--only" and i + 1 < len(sys.argv) - 1:
        only = sys.argv[i + 2]

# ── Load env ──────────────────────────────────────────────────────────────────
def env_file_path() -> Path:
    if path := os.environ.get("AXON_ENV_FILE"):
        return Path(path).expanduser()
    axon_home = Path(os.environ.get("AXON_HOME", "~/.axon")).expanduser()
    canonical = axon_home / ".env"
    return canonical if canonical.exists() else REPO / ".env"


env = os.environ.copy()
ENV_FILE = env_file_path()
if ENV_FILE.exists():
    for line in ENV_FILE.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, _, v = line.partition("=")
        env.setdefault(k.strip(), v.strip().strip('"').strip("'"))

# ── Load export ───────────────────────────────────────────────────────────────
with open(EXPORT) as f:
    data = json.load(f)
seeds = data["rebuild_seeds"]

# Build include_source map: latest request per repo
include_source: dict[str, bool] = {}
for req in seeds["github_requests"]:
    repo = req["target"]
    if repo not in include_source:
        include_source[repo] = req["options"].get("include_source", False)


def run(cmd: list[str], label: str) -> None:
    if dry_run:
        print(f"  [dry] {' '.join(cmd)}")
        return
    result = subprocess.run(cmd, env=env, capture_output=True, text=True)
    if result.returncode != 0:
        print(
            f"  WARN [{label}] exit={result.returncode}: {result.stderr.strip()[:120]}"
        )
    time.sleep(0.05)  # 50ms between enqueues — avoids hammering AMQP


def section(title: str) -> None:
    print(f"\n{'─' * 60}")
    print(f"  {title}")
    print(f"{'─' * 60}")


# ── Crawl seeds ───────────────────────────────────────────────────────────────
if not only or only == "crawl":
    crawl_urls = seeds["crawl_seed_urls"]
    section(f"Crawl seeds ({len(crawl_urls)} URLs)")
    for i, url in enumerate(crawl_urls, 1):
        print(f"  [{i:3}/{len(crawl_urls)}] crawl {url}")
        run([str(BINARY), "crawl", url], f"crawl:{url}")

# ── GitHub ingest ─────────────────────────────────────────────────────────────
if not only or only == "github":
    github_repos = seeds["github_repos"]
    section(f"GitHub repos ({len(github_repos)} repos)")
    for i, repo in enumerate(github_repos, 1):
        inc = include_source.get(repo, False)
        cmd = [str(BINARY), "ingest", repo]
        if not inc:
            cmd.append("--no-source")
        flag = "+src" if inc else "-src"
        print(f"  [{i:3}/{len(github_repos)}] ingest {repo} [{flag}]")
        run(cmd, f"github:{repo}")

# ── YouTube ingest ────────────────────────────────────────────────────────────
if not only or only == "youtube":
    youtube_targets = seeds["youtube_targets"]
    section(f"YouTube ({len(youtube_targets)} targets)")
    for i, url in enumerate(youtube_targets, 1):
        print(f"  [{i}/{len(youtube_targets)}] ingest {url}")
        run([str(BINARY), "ingest", url], f"youtube:{url}")

# ── Summary ───────────────────────────────────────────────────────────────────
section("Done")
crawl_count = len(seeds["crawl_seed_urls"]) if not only or only == "crawl" else 0
github_count = len(seeds["github_repos"]) if not only or only == "github" else 0
youtube_count = len(seeds["youtube_targets"]) if not only or only == "youtube" else 0
total = crawl_count + github_count + youtube_count
print(
    f"  Queued: {crawl_count} crawl + {github_count} github + {youtube_count} youtube = {total} jobs"
)
if dry_run:
    print("  (dry run — no commands were executed)")
