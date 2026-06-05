#!/usr/bin/env python3
"""Run a filtered cargo test command only if the filter matches tests."""

from __future__ import annotations

import subprocess
import sys


def split_cargo_args(argv: list[str]) -> tuple[list[str], list[str]]:
    if "--" not in argv:
        return argv, []
    index = argv.index("--")
    return argv[:index], argv[index + 1 :]


def list_command(cargo_args: list[str]) -> list[str]:
    return cargo_args + ["--", "--list"]


def count_listed_tests(output: str) -> int:
    return sum(1 for line in output.splitlines() if line.rstrip().endswith(": test"))


def main(argv: list[str]) -> int:
    if argv[:1] == ["--"]:
        argv = argv[1:]
    if len(argv) < 3 or argv[0:2] != ["cargo", "test"]:
        print(
            "usage: cargo_test_filter_guard.py -- cargo test [cargo test args...]",
            file=sys.stderr,
        )
        return 2

    cargo_args, test_args = split_cargo_args(argv)
    listed = subprocess.run(
        list_command(cargo_args),
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if listed.returncode != 0:
        print(listed.stdout, end="")
        return listed.returncode

    matches = count_listed_tests(listed.stdout)
    if matches == 0:
        print(
            f"cargo test filter matched 0 tests: {' '.join(cargo_args)}",
            file=sys.stderr,
        )
        return 1

    print(f"cargo test filter matched {matches} test(s)")
    return subprocess.run(cargo_args + (["--"] + test_args if test_args else [])).returncode


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
