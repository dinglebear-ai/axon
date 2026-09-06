#!/usr/bin/env python3
"""Entrypoint wrapper for qdrant quality checks."""

from __future__ import annotations

import sys
from datetime import UTC

from qdrant_quality_analysis import (
    canonicalize_url_for_dedupe,
    parse_payload_timestamp,
    path_prefix_excluded,
)
from qdrant_quality_impl import main
from qdrant_quality_reporting import confirm_destructive_action

__all__ = [
    "canonicalize_url_for_dedupe",
    "confirm_destructive_action",
    "main",
    "parse_payload_timestamp",
    "path_prefix_excluded",
    "UTC",
]


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print('\nInterrupted', file=sys.stderr)
        raise SystemExit(130)
    except Exception as exc:  # noqa: BLE001
        print(f'Error: {exc}', file=sys.stderr)
        raise SystemExit(1)
