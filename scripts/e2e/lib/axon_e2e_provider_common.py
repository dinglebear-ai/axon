"""Shared imports and errors for the E2E provider adapter modules."""

from __future__ import annotations

import hashlib
import json
import os
import re
import socket
import sqlite3
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any


class ProviderError(RuntimeError):
    pass


def segment(value: str) -> str:
    return urllib.parse.quote(value, safe="")
