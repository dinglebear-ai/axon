#!/usr/bin/env python3
"""Fail CI/pre-commit when newly changed files/functions become monolithic."""

from __future__ import annotations

import argparse
import sys

from enforce_monoliths_helpers import (
    CHECKABLE_EXTENSIONS,
    DEFAULT_ALLOWLIST_EXPIRY_DAYS,
    DEFAULT_FILE_MAX_LINES,
    DEFAULT_FUNCTION_MAX_LINES,
    DEFAULT_FUNCTION_WARN_LINES,
    REPO_ROOT,
    RUST_EXTENSIONS,
    changed_files,
    check_allowlist_expiry,
    count_effective_c_like_lines,
    count_effective_lines,
    count_effective_python_lines,
    file_line_count,
    file_line_count_from_text,
    is_excluded,
    is_text_file,
    load_allowlist,
    normalize_file_arg,
    parse_changed_line_numbers,
    parse_rust_functions,
    validate_ref_exists,
)


def _self_test() -> int:
    tests = []

    tests.append(
        (
            "c_like_comments",
            count_effective_c_like_lines(
                [
                    "let a = 1; // trailing",
                    "/* block start",
                    "still block */ let b = 2;",
                    'let s = "http://x"; // url',
                    "   ",
                    "// comment only",
                ]
            )
            == 3,
        )
    )

    tests.append(
        (
            "python_docstrings",
            count_effective_python_lines(
                '"""module docs"""\n\n'
                "def f():\n"
                '    """function docs"""\n'
                "    x = 1\n"
                "    return x\n"
            )
            == 3,
        )
    )

    tests.append(
        (
            "hash_comments",
            count_effective_lines(["# c1", "x=1", "   ", "echo hi # inline"], ".sh")
            == 2,
        )
    )

    tests.append(
        (
            "cfg_test_decl_excluded",
            file_line_count_from_text(
                "#[cfg(test)]\nmod tests;\nfn real() {\n 1\n}\n",
                ".rs",
            )
            == 3,
        )
    )

    tests.append(
        (
            "unknown_suffix_fallback",
            count_effective_lines(["", "a", "# x"], ".md") == 2,
        )
    )

    failed = [name for name, ok in tests if not ok]
    if failed:
        print("self-test failed:", ", ".join(failed), file=sys.stderr)
        return 1
    print("self-test passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", help="Base git ref")
    parser.add_argument("--head", help="Head git ref")
    parser.add_argument(
        "--file", help="Check a single repo-relative or absolute file path"
    )
    parser.add_argument(
        "--staged",
        action="store_true",
        help="Use staged changes from git index (for local pre-commit)",
    )
    parser.add_argument(
        "--self-test", action="store_true", help="Run internal detector self-tests"
    )
    parser.add_argument(
        "--whole-repo",
        action="store_true",
        help=(
            "Informational whole-repo size report (always exits 0). "
            "Skips allowlisted files unless --include-allowlisted is set."
        ),
    )
    parser.add_argument(
        "--include-allowlisted",
        action="store_true",
        help="With --whole-repo, also include files already in the allowlist.",
    )
    parser.add_argument("--file-max-lines", type=int, default=DEFAULT_FILE_MAX_LINES)
    parser.add_argument(
        "--function-warn-lines", type=int, default=DEFAULT_FUNCTION_WARN_LINES
    )
    parser.add_argument(
        "--function-max-lines", type=int, default=DEFAULT_FUNCTION_MAX_LINES
    )
    parser.add_argument(
        "--allowlist-expiry-days",
        type=int,
        default=DEFAULT_ALLOWLIST_EXPIRY_DAYS,
        help="Max days an allowlist entry can live before it must be resolved",
    )
    args = parser.parse_args()

    if args.self_test:
        return _self_test()

    if args.whole_repo:
        from enforce_monoliths_report import whole_repo_report

        return whole_repo_report(
            file_max=args.file_max_lines,
            fn_warn=args.function_warn_lines,
            fn_max=args.function_max_lines,
            include_allowlisted=args.include_allowlisted,
        )

    if not args.file and not args.staged and (not args.base or not args.head):
        print(
            "provide --file, --staged, or both --base and --head",
            file=sys.stderr,
        )
        return 2

    try:
        if args.file:
            rel = normalize_file_arg(args.file)
            files = [rel]
        elif not args.staged:
            validate_ref_exists(args.base)
            validate_ref_exists(args.head)
            files = changed_files(args.base, args.head, args.staged)
        else:
            files = changed_files(args.base, args.head, args.staged)
        allowlist = load_allowlist()
    except RuntimeError as exc:
        print(f"monolith check setup failed: {exc}", file=sys.stderr)
        return 2

    violations: list[str] = []
    warnings: list[str] = []

    for path in files:
        if is_excluded(path, allowlist):
            continue

        full = REPO_ROOT / path
        if not is_text_file(full):
            continue
        if full.suffix not in CHECKABLE_EXTENSIONS:
            continue

        line_count = file_line_count(full)
        if line_count > args.file_max_lines:
            violations.append(
                f"FILE {path}: {line_count} lines (limit {args.file_max_lines})"
            )

        if full.suffix not in RUST_EXTENSIONS:
            continue

        rust_lines = full.read_text(encoding="utf-8", errors="ignore").splitlines()
        if args.file:
            changed_lines = set(range(1, len(rust_lines) + 1))
        else:
            changed_lines = parse_changed_line_numbers(
                args.base, args.head, path, args.staged
            )
            if not changed_lines:
                continue

        for fn in parse_rust_functions(full):
            if not any(fn.start <= ln <= fn.end for ln in changed_lines):
                continue
            fn_len = count_effective_lines(rust_lines[fn.start - 1 : fn.end], ".rs")
            if fn_len > args.function_max_lines:
                violations.append(
                    "FUNCTION "
                    f"{path}:{fn.start} {fn.name}() is {fn_len} lines "
                    f"(limit {args.function_max_lines})"
                )
            elif fn_len > args.function_warn_lines:
                warnings.append(
                    "FUNCTION "
                    f"{path}:{fn.start} {fn.name}() is {fn_len} lines "
                    f"(warning {args.function_warn_lines}, limit {args.function_max_lines})"
                )

    # Check allowlist expiry (always, even in --file mode)
    expiry_violations = check_allowlist_expiry(args.allowlist_expiry_days)
    violations.extend(expiry_violations)

    if warnings:
        print("Monolith policy warnings:")
        for item in warnings:
            print(f"  - {item}")

    if violations:
        print("Monolith policy violations found:")
        for item in violations:
            print(f"  - {item}")
        print("\nSplit the file — do not add allowlist exceptions.")
        return 1

    print("Monolith policy check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
