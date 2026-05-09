#!/usr/bin/env python3
"""Audit image references in tracked Docker Compose files."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path


DEFAULT_COMPOSE_FILES = ("docker-compose.yaml",)


def image_tag(image: str) -> str | None:
    tail = image.rsplit("/", 1)[-1]
    if ":" not in tail:
        return None
    return tail.rsplit(":", 1)[-1]


def load_compose(files: list[str]) -> dict:
    cmd = ["docker", "compose"]
    for file in files:
        cmd.extend(["-f", file])
    cmd.extend(["config", "--format", "json"])
    result = subprocess.run(cmd, check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


def audit(files: list[str]) -> int:
    missing = [file for file in files if not Path(file).exists()]
    if missing:
        for file in missing:
            print(f"missing compose file: {file}", file=sys.stderr)
        return 2

    config = load_compose(files)
    services = config.get("services", {})
    rows: list[tuple[str, str, str, str]] = []
    exit_code = 0

    for name, service in sorted(services.items()):
        image = service.get("image")
        if not image:
            rows.append((name, "-", "build-only", "no image reference"))
            continue
        registry = image.split("/", 1)[0] if "." in image.split("/", 1)[0] else "docker.io"
        tag = image_tag(image)
        status = "ok"
        if tag is None:
            status = "untagged"
            exit_code = 1
        elif tag == "latest":
            status = "latest"
            exit_code = 1
        rows.append((name, registry, status, image))

    print("service\tregistry\tstatus\timage")
    for row in rows:
        print("\t".join(row))
    return exit_code


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "-f",
        "--file",
        action="append",
        dest="files",
        help="compose file to audit; repeatable",
    )
    args = parser.parse_args()
    return audit(args.files or list(DEFAULT_COMPOSE_FILES))


if __name__ == "__main__":
    raise SystemExit(main())
