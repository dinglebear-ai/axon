#!/usr/bin/env python3
"""Construct the deterministic local Git fixture without any network access."""

from __future__ import annotations

import argparse
import os
import subprocess
from pathlib import Path


def build(destination: Path) -> str:
    destination.mkdir(parents=True)
    (destination / "README.md").write_text("# Axon deterministic Git fixture\n", encoding="utf-8")
    (destination / "document.txt").write_text("canonical fixture alpha beta\n", encoding="utf-8")
    env = os.environ.copy()
    env.update({
        "GIT_AUTHOR_NAME": "Axon E2E", "GIT_AUTHOR_EMAIL": "e2e@axon.invalid",
        "GIT_COMMITTER_NAME": "Axon E2E", "GIT_COMMITTER_EMAIL": "e2e@axon.invalid",
        "GIT_AUTHOR_DATE": "2020-01-02T03:04:05Z", "GIT_COMMITTER_DATE": "2020-01-02T03:04:05Z",
    })
    subprocess.run(["git", "init", "-q", "-b", "main"], cwd=destination, env=env, check=True)
    subprocess.run(["git", "add", "README.md", "document.txt"], cwd=destination, env=env, check=True)
    subprocess.run(["git", "commit", "-q", "-m", "deterministic fixture"], cwd=destination, env=env, check=True)
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=destination, env=env, text=True).strip()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(); parser.add_argument("destination", type=Path)
    print(build(parser.parse_args().destination))
