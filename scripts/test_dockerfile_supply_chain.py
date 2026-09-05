#!/usr/bin/env python3
"""Contract checks for production Docker build provenance."""

from pathlib import Path
import re

dockerfile = Path(__file__).resolve().parents[1] / "config" / "Dockerfile"
text = dockerfile.read_text(encoding="utf-8")

if "groupadd --system --gid 1000" in text or "useradd  --system --uid 1000" in text:
    raise SystemExit("runtime image must reuse the base image's numeric 1000:1000 account")

for line in text.splitlines():
    if line.startswith("FROM "):
        image = line.split()[1]
        if image != "scratch" and "@sha256:" not in image:
            raise SystemExit(f"unpinned Docker base image: {line}")

if re.search(r"(?:curl|wget)[^\n|]*\|\s*(?:ba)?sh", text):
    raise SystemExit("remote content must not be piped to a shell in config/Dockerfile")

if "rust:1.97.1-bookworm@sha256:" not in text:
    raise SystemExit("builder image must match workspace rust-version 1.97.1")

print("ok - Dockerfile supply-chain contract passed")
