#!/usr/bin/env python3
"""Configuration helpers for qdrant quality checks."""

from __future__ import annotations

import os
from pathlib import Path

from qdrant_quality_models import NormalizedExcludePrefixes


def load_dotenv_file() -> dict[str, str]:
    """Parse canonical Axon env file into key/value pairs."""
    if env_override := os.getenv("AXON_ENV_FILE"):
        env_path = Path(env_override).expanduser()
    else:
        canonical = Path(os.getenv("AXON_HOME", "~/.axon")).expanduser() / ".env"
        repo_env = Path(__file__).resolve().parents[1] / ".env"
        env_path = canonical if canonical.exists() else repo_env
    values: dict[str, str] = {}
    if not env_path.exists():
        return values

    for raw_line in env_path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if not key:
            continue
        if value and ((value[0] == value[-1]) and value[0] in {"'", '"'}):
            value = value[1:-1]
        values[key] = value

    return values


DOTENV_VALUES = load_dotenv_file()
DEFAULT_QDRANT_URL = os.getenv("QDRANT_URL", DOTENV_VALUES.get("QDRANT_URL", "http://localhost:53333")).rstrip("/")
DEFAULT_COLLECTION = os.getenv("QDRANT_COLLECTION", DOTENV_VALUES.get("QDRANT_COLLECTION", "cortex"))

# Must stay aligned with src/core/config.rs::default_exclude_prefixes().
DEFAULT_EXCLUDE_PREFIXES = [
    "/fr",
    "/de",
    "/es",
    "/ja",
    "/zh",
    "/zh-cn",
    "/zh-tw",
    "/ko",
    "/pt",
    "/pt-br",
    "/it",
    "/nl",
    "/pl",
    "/ru",
    "/tr",
    "/ar",
    "/id",
    "/vi",
    "/th",
    "/cs",
    "/da",
    "/fi",
    "/no",
    "/sv",
    "/he",
    "/uk",
    "/ro",
    "/hu",
    "/el",
]


def parse_csv_values(raw: str) -> list[str]:
    return [part.strip() for part in raw.split(",")]


def normalize_exclude_prefixes(values: list[str]) -> NormalizedExcludePrefixes:
    disable_by_empty = len(values) == 1 and values[0].strip() in {"", "/"}
    disable_by_none = any(v.strip().lower() == "none" for v in values)
    if disable_by_none:
        return NormalizedExcludePrefixes(prefixes=[], disable_defaults=True)

    out: list[str] = []
    for raw in values:
        trimmed = raw.strip()
        if not trimmed or trimmed == "/":
            continue
        out.append(trimmed if trimmed.startswith("/") else f"/{trimmed}")

    out = sorted(set(out))
    return NormalizedExcludePrefixes(prefixes=out, disable_defaults=disable_by_empty)
