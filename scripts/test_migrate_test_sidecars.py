#!/usr/bin/env python3
"""Focused regressions for the sidecar migration parser."""

import importlib.util
from pathlib import Path
import subprocess
import sys
import time
import unittest


SCRIPT = Path(__file__).with_name("migrate_test_sidecars.py")
SPEC = importlib.util.spec_from_file_location("migrate_test_sidecars", SCRIPT)
assert SPEC and SPEC.loader
MIGRATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MIGRATE)


class SidecarParserTests(unittest.TestCase):
    def test_finds_nested_cfg_test_and_skips_non_test_cfg(self):
        source = """
#[cfg(all(test, unix))]
#[allow(unsafe_code)]
mod tests { #[test] fn works() {} }

#[cfg(feature = "test-support")]
mod support { fn helper() {} }
"""
        blocks = MIGRATE.find_inline_blocks(source)
        self.assertEqual(len(blocks), 1)
        self.assertEqual(blocks[0][2], "tests")
        self.assertEqual(blocks[0][3], "#[cfg(all(test, unix))]")

        multiline = "#[cfg(all(\n  test,\n  unix\n))]\nmod tests {}"
        self.assertEqual(len(MIGRATE.find_inline_blocks(multiline)), 1)

        quoted = '#[cfg(test)]\nmod tests { let a = "}"; let b = r#"{"#; /* } */ }'
        blocks = MIGRATE.find_inline_blocks(quoted)
        self.assertEqual(len(blocks), 1)
        self.assertIn('let b = r#"{"#', blocks[0][-1])

        for malformed in (
            '#[cfg(test)]\nmod tests { let value = "unterminated',
            "#[cfg(test)]\nmod tests { /* unterminated",
            "#[cfg(test)]\nmod tests { fn missing_close() {",
        ):
            self.assertEqual(MIGRATE.find_inline_blocks(malformed), [])

    def test_long_malformed_attribute_is_processed_in_linear_time(self):
        cases = [
            "#[cfg(" + ("'" * 200_000) + "\nmod tests {",
            "#[cfg(test)]" + (" " * 200_000),
            "#[cfg(test)]\n#[allow(" + (" " * 200_000),
        ]
        for source in cases:
            with self.subTest(prefix=source[:20]):
                started = time.monotonic()
                self.assertEqual(MIGRATE.find_inline_blocks(source), [])
                self.assertLess(time.monotonic() - started, 0.5)

        code = f"""
import importlib.util
spec = importlib.util.spec_from_file_location('migration', {str(SCRIPT)!r})
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
assert module.find_inline_blocks(('#[cfg(test)]\\n' * 20000)) == []
"""
        subprocess.run([sys.executable, "-c", code], check=True, timeout=1)


if __name__ == "__main__":
    unittest.main()
