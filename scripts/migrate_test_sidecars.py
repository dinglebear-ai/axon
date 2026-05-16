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

import re
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent


def find_inline_blocks(content: str) -> list[tuple[int, int, str, str, str]]:
    """
    Return list of (start, end, mod_name, cfg_gate, body) for inline cfg(test) mod blocks.
    start/end are character positions in content.
    cfg_gate is the full #[cfg(...)] attribute (e.g. '#[cfg(test)]' or '#[cfg(all(test, unix))]')
    """
    results = []
    # Match cfg gate variants, then optional attributes, then mod X {
    # Use a balanced-paren approach for the cfg attribute to handle nested parens
    # e.g. #[cfg(all(test, unix))] where [^)]* would stop at the inner ')'.
    pattern = re.compile(
        r'(#\[cfg\((?:[^()]*|\([^()]*\))*\btest\b(?:[^()]*|\([^()]*\))*\)\])'  # cfg gate (balanced)
        r'(\s*(?:#\[[^\]]*\]\s*)*)'            # optional other attributes between cfg and mod
        r'\s*\n\s*mod\s+(\w+)\s*\{',           # mod name {
        re.MULTILINE
    )
    for m in pattern.finditer(content):
        cfg_gate = m.group(1)
        intermediate_attrs = m.group(2).strip()  # e.g. '#[allow(unsafe_code)]'
        mod_name = m.group(3)
        brace_start = m.end() - 1  # position of the opening {
        # Find matching closing }
        depth = 1
        i = brace_start + 1
        while i < len(content) and depth > 0:
            if content[i] == '{':
                depth += 1
            elif content[i] == '}':
                depth -= 1
            i += 1
        block_end = i  # one past the closing }
        block_start = m.start()
        body = content[brace_start + 1:i - 1]  # content between { and }
        results.append((block_start, block_end, mod_name, cfg_gate, intermediate_attrs, body))
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
        paths = [Path(p) for p in specific]
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
