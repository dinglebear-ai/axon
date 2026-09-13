#!/usr/bin/env python3
"""
Migrate inline #[cfg(test)] mod X { ... } blocks to sibling _tests.rs files.

For each source file:
  - If a sidecar already exists, add the #[path] declaration to source (remove inline block)
  - If no sidecar, create it from the inline block body and add the #[path] declaration

Pattern per lon7.1 foundation:
  In source file: #[cfg(test)] #[path = "foo_tests.rs"] mod tests;
  File on disk:   foo_tests.rs (sibling to foo.rs)
"""

import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent


def cfg_mentions_test(attribute: str) -> bool:
    """Return whether a cfg attribute contains the `test` predicate outside strings."""
    in_string = False
    escaped = False
    identifier = []
    for char in attribute:
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
        elif char.isalnum() or char == "_":
            identifier.append(char)
        else:
            if "".join(identifier) == "test":
                return True
            identifier.clear()
    return "".join(identifier) == "test"


def rust_attribute_end(content: str, start: int) -> int | None:
    """Return one-past `]` for a Rust attribute, ignoring brackets in strings."""
    if not content.startswith("#[", start):
        return None
    in_string = False
    escaped = False
    for index in range(start + 2, len(content)):
        char = content[index]
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
        elif char == '"':
            in_string = True
        elif char == "]":
            return index + 1
    return None


def skip_whitespace(content: str, index: int) -> int:
    while index < len(content) and content[index].isspace():
        index += 1
    return index


def rust_block_end(content: str, brace_start: int) -> int | None:
    """Find a Rust block's end while ignoring braces in strings and comments."""
    depth = 1
    index = brace_start + 1
    while index < len(content) and depth:
        if content.startswith("//", index):
            newline = content.find("\n", index + 2)
            index = len(content) if newline == -1 else newline + 1
            continue
        if content.startswith("/*", index):
            comment_depth = 1
            index += 2
            while index < len(content) and comment_depth:
                if content.startswith("/*", index):
                    comment_depth += 1
                    index += 2
                elif content.startswith("*/", index):
                    comment_depth -= 1
                    index += 2
                else:
                    index += 1
            continue
        if content[index] == 'r':
            marker = index + 1
            while marker < len(content) and content[marker] == "#":
                marker += 1
            if marker < len(content) and content[marker] == '"':
                terminator = '"' + ("#" * (marker - index - 1))
                end = content.find(terminator, marker + 1)
                index = len(content) if end == -1 else end + len(terminator)
                continue
        if content[index] == '"':
            index += 1
            while index < len(content):
                if content[index] == "\\":
                    index += 2
                elif content[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
            continue
        if content[index] == "'":
            # A character literal has a nearby closing quote; a Rust lifetime does not.
            closing = index + 1
            escaped = False
            while closing < min(len(content), index + 14):
                char = content[closing]
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == "'":
                    index = closing + 1
                    break
                elif char.isspace():
                    break
                closing += 1
            else:
                index += 1
            if index == closing + 1:
                continue
        if content[index] == "{":
            depth += 1
        elif content[index] == "}":
            depth -= 1
        index += 1
    return index if depth == 0 else None


def find_inline_blocks(content: str) -> list[tuple[int, int, str, str, str, str]]:
    """
    Return (start, end, mod_name, cfg_gate, intermediate_attrs, body) tuples.
    start/end are character positions in content.
    cfg_gate is the full #[cfg(...)] attribute (e.g. '#[cfg(test)]' or '#[cfg(all(test, unix))]')
    """
    results = []
    search_from = 0
    while (block_start := content.find("#[cfg(", search_from)) != -1:
        cfg_end = rust_attribute_end(content, block_start)
        if cfg_end is None:
            break
        search_from = cfg_end
        cfg_gate = content[block_start:cfg_end]
        if not cfg_mentions_test(cfg_gate):
            continue

        index = skip_whitespace(content, cfg_end)
        attrs_start = index
        while content.startswith("#[", index):
            attr_end = rust_attribute_end(content, index)
            if attr_end is None:
                index = len(content)
                break
            index = skip_whitespace(content, attr_end)
        intermediate_attrs = content[attrs_start:index].strip()

        if not content.startswith("mod", index):
            search_from = max(search_from, index)
            continue
        index += 3
        if index >= len(content) or not content[index].isspace():
            search_from = index
            continue
        index = skip_whitespace(content, index)
        name_start = index
        while index < len(content) and (content[index].isalnum() or content[index] == "_"):
            index += 1
        if index == name_start:
            search_from = index
            continue
        mod_name = content[name_start:index]
        index = skip_whitespace(content, index)
        if index >= len(content) or content[index] != "{":
            search_from = index
            continue
        brace_start = index
        block_end = rust_block_end(content, brace_start)
        if block_end is None:
            search_from = len(content)
            continue
        body = content[brace_start + 1:block_end - 1]
        results.append((block_start, block_end, mod_name, cfg_gate, intermediate_attrs, body))
        search_from = block_end
    return results


def sidecar_path(source: Path, mod_name: str) -> Path:
    """Return path of the sidecar file for a given source and mod name.

    Convention (from CLAUDE.md + lon7.1 examples):
      mod tests             → foo_tests.rs
      mod decode_tests      → foo_decode_tests.rs  (mod_name ends in _tests — no double suffix)
      mod proptest_tests    → foo_proptest_tests.rs (same)
      mod legacy            → foo_legacy_tests.rs  (mod_name doesn't end in _tests — add suffix)
      mod integration_tests → foo_integration_tests.rs (ends in _tests — no double suffix)
    """
    stem = source.stem
    if mod_name == "tests":
        sidecar_name = f"{stem}_tests.rs"
    elif mod_name.endswith("_tests"):
        sidecar_name = f"{stem}_{mod_name}.rs"
    else:
        sidecar_name = f"{stem}_{mod_name}_tests.rs"
    return source.parent / sidecar_name


def path_attr(source: Path, mod_name: str) -> str:
    """Return the #[path] attribute string for the sidecar."""
    sc = sidecar_path(source, mod_name)
    return sc.name  # relative to source file's directory


def migrate_file(source: Path, dry_run: bool = False) -> bool:
    """Migrate all inline cfg(test) blocks in source. Returns True if any change made."""
    content = source.read_text(encoding="utf-8")
    blocks = find_inline_blocks(content)
    if not blocks:
        return False

    changes_made = False
    # Process in reverse order to preserve character positions
    for start, end, mod_name, cfg_gate, intermediate_attrs, body in reversed(blocks):
        sc_path = sidecar_path(source, mod_name)
        attr = path_attr(source, mod_name)

        # Build sidecar content from body (strip common leading whitespace)
        body_lines = body.split('\n')
        # Remove first empty line if present
        if body_lines and not body_lines[0].strip():
            body_lines = body_lines[1:]
        # Remove last empty line if present
        if body_lines and not body_lines[-1].strip():
            body_lines = body_lines[:-1]
        # Dedent: find minimum indentation
        non_empty = [l for l in body_lines if l.strip()]
        if non_empty:
            min_indent = min(len(l) - len(l.lstrip()) for l in non_empty)
            body_lines = [l[min_indent:] if len(l) >= min_indent else l for l in body_lines]
        sidecar_content = '\n'.join(body_lines).rstrip('\n') + '\n'

        if sc_path.exists():
            # Sidecar already exists — just update source to point to it
            if not dry_run:
                existing = sc_path.read_text(encoding="utf-8")
                # Don't overwrite if it looks correct (has test fns)
                if "#[test]" not in existing and "#[tokio::test]" not in existing:
                    sc_path.write_text(sidecar_content, encoding="utf-8")
                    print(f"  UPDATED sidecar: {sc_path.relative_to(PROJECT_ROOT)}")
                else:
                    print(f"  KEPT existing sidecar: {sc_path.relative_to(PROJECT_ROOT)}")
            else:
                print(f"  DRY-RUN: would use existing sidecar {sc_path.name}")
        else:
            # Create new sidecar
            if not dry_run:
                sc_path.write_text(sidecar_content, encoding="utf-8")
                print(f"  CREATED sidecar: {sc_path.relative_to(PROJECT_ROOT)}")
            else:
                print(f"  DRY-RUN: would create {sc_path.name}")

        # Replace inline block in source with #[path] declaration.
        # Preserve any intermediate attributes (e.g. #[allow(unsafe_code)]).
        if intermediate_attrs:
            replacement = f"{cfg_gate}\n{intermediate_attrs}\n#[path = \"{attr}\"]\nmod {mod_name};"
        else:
            replacement = f"{cfg_gate}\n#[path = \"{attr}\"]\nmod {mod_name};"
        content = content[:start] + replacement + content[end:]
        changes_made = True

    if changes_made and not dry_run:
        source.write_text(content, encoding="utf-8")
        print(f"  UPDATED source: {source.relative_to(PROJECT_ROOT)}")

    return changes_made


def main():
    dry_run = "--dry-run" in sys.argv
    check_mode = "--check" in sys.argv
    specific = [a for a in sys.argv[1:] if not a.startswith("--")]

    paths: list[Path] = []
    if specific:
        paths = [Path(p).resolve() for p in specific]
    else:
        # Scan all .rs files in src/ and xtask/, skip _tests.rs and tests.rs
        for root in [PROJECT_ROOT / "src", PROJECT_ROOT / "xtask"]:
            for p in root.rglob("*.rs"):
                name = p.name
                if name.endswith("_tests.rs") or name == "tests.rs" or name == "proptest_tests.rs":
                    continue
                paths.append(p)

    if check_mode:
        # Check mode: fail non-zero if any inline blocks remain (CI guard).
        remaining = 0
        for source in sorted(paths):
            content = source.read_text(encoding="utf-8", errors="ignore")
            blocks = find_inline_blocks(content)
            if blocks:
                remaining += len(blocks)
                for _, _, mod_name, cfg_gate, _, _ in blocks:
                    print(f"INLINE: {source.relative_to(PROJECT_ROOT)}: {cfg_gate} mod {mod_name}")
        if remaining:
            print(f"\nFAIL: {remaining} inline test block(s) remain. Run migrate_test_sidecars.py to fix.")
            sys.exit(1)
        print(f"OK: no inline #[cfg(test)] mod blocks found in {len(paths)} files")
        return

    migrated = 0
    skipped = 0
    for source in sorted(paths):
        changed = migrate_file(source, dry_run=dry_run)
        if changed:
            migrated += 1
        else:
            skipped += 1

    print(f"\n{'DRY-RUN ' if dry_run else ''}Summary: {migrated} files migrated, {skipped} files unchanged")


if __name__ == "__main__":
    main()
