#!/usr/bin/env python3
"""Reject docker compose port mappings that bind to a host/interface."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


DEFAULT_FILES = ("docker-compose.yaml", "docker-compose.yml")


def strip_inline_comment(value: str) -> str:
    quote: str | None = None
    escaped = False
    result: list[str] = []

    for char in value:
        if escaped:
            result.append(char)
            escaped = False
            continue
        if char == "\\" and quote == '"':
            result.append(char)
            escaped = True
            continue
        if char in ("'", '"'):
            if quote is None:
                quote = char
            elif quote == char:
                quote = None
            result.append(char)
            continue
        if char == "#" and quote is None:
            break
        result.append(char)

    return "".join(result).strip()


def unquote(value: str) -> str:
    if len(value) >= 2 and value[0] == value[-1] and value[0] in ("'", '"'):
        return value[1:-1]
    return value


def split_top_level_colons(value: str) -> list[str]:
    parts: list[str] = []
    start = 0
    env_depth = 0
    bracket_depth = 0
    index = 0

    while index < len(value):
        if value.startswith("${", index):
            env_depth += 1
            index += 2
            continue
        char = value[index]
        if env_depth:
            if char == "}":
                env_depth -= 1
            index += 1
            continue
        if char == "[":
            bracket_depth += 1
        elif char == "]" and bracket_depth:
            bracket_depth -= 1
        elif char == ":" and bracket_depth == 0:
            parts.append(value[start:index])
            start = index + 1
        index += 1

    parts.append(value[start:])
    return parts


def env_defaults(value: str) -> list[str]:
    defaults: list[str] = []
    index = 0
    while index < len(value):
        start = value.find("${", index)
        if start == -1:
            break
        end = value.find("}", start + 2)
        if end == -1:
            break
        body = value[start + 2 : end]
        marker = ":-"
        if marker in body:
            defaults.append(body.split(marker, 1)[1])
        index = end + 1
    return defaults


def mapping_errors(value: str) -> list[str]:
    errors: list[str] = []
    port_part = value.split("/", 1)[0]
    fields = split_top_level_colons(port_part)

    if len(fields) >= 3:
        errors.append("short syntax includes a host/interface prefix")

    for default in env_defaults(value):
        if ":" in default:
            errors.append(
                f"environment default {default!r} includes ':' and can bind a host/interface"
            )

    return errors


def find_errors(path: str, lines: list[str]) -> list[str]:
    errors: list[str] = []
    ports_indent: int | None = None

    for line_number, raw_line in enumerate(lines, 1):
        stripped = raw_line.strip()
        if not stripped or stripped.startswith("#"):
            continue

        indent = len(raw_line) - len(raw_line.lstrip(" "))
        if ports_indent is not None and indent <= ports_indent:
            ports_indent = None

        if stripped == "ports:":
            ports_indent = indent
            continue

        if ports_indent is None or indent <= ports_indent:
            continue

        content = strip_inline_comment(stripped)
        if not content:
            continue

        if content.startswith("- "):
            item = content[2:].strip()
            if item.startswith("host_ip:"):
                errors.append(f"{path}:{line_number}: ports long syntax must not set host_ip")
                continue
            if not item or item.endswith(":"):
                continue
            value = unquote(item)
            for reason in mapping_errors(value):
                errors.append(f"{path}:{line_number}: {reason}: {value!r}")
            continue

        if content.startswith("host_ip:"):
            errors.append(f"{path}:{line_number}: ports long syntax must not set host_ip")

    return errors


def staged_content(path: str) -> list[str] | None:
    result = subprocess.run(
        ["git", "show", f":{path}"],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    if result.returncode != 0:
        return None
    return result.stdout.splitlines()


def changed_compose_files() -> list[str]:
    result = subprocess.run(
        [
            "git",
            "diff",
            "--cached",
            "--name-only",
            "--",
            "docker-compose*.yaml",
            "docker-compose*.yml",
            "*.compose.yaml",
            "*.compose.yml",
        ],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    if result.returncode != 0:
        return []
    return [line for line in result.stdout.splitlines() if line]


def file_content(path: str, staged: bool) -> list[str] | None:
    if staged:
        content = staged_content(path)
        if content is not None:
            return content

    disk_path = Path(path)
    if not disk_path.exists() or not disk_path.is_file():
        return None
    return disk_path.read_text(encoding="utf-8").splitlines()


def resolve_paths(args: argparse.Namespace) -> list[str]:
    if args.files:
        return args.files
    if args.staged:
        changed = changed_compose_files()
        if changed:
            return changed
    return [path for path in DEFAULT_FILES if Path(path).exists()]


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Block docker compose ports entries that include host/interface bindings."
    )
    parser.add_argument("--staged", action="store_true", help="prefer staged file content")
    parser.add_argument("files", nargs="*", help="compose files to check")
    args = parser.parse_args()

    errors: list[str] = []
    for path in resolve_paths(args):
        content = file_content(path, args.staged)
        if content is None:
            continue
        errors.extend(find_errors(path, content))

    if errors:
        print("docker compose port binding check failed:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        print(
            'Use bare host-port mappings like "52000:80"; do not use '
            '"127.0.0.1:52000:80", "localhost:52000:80", or any other host prefix.',
            file=sys.stderr,
        )
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
