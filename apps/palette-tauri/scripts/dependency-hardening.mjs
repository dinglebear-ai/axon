import assert from "node:assert/strict";
import { mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

// Resolve the packages actually consumed by shadcn through pnpm's dependency
// graph, rather than an unrelated top-level copy that could hide a bad override.
const requireFromShadcn = createRequire(
  realpathSync(new URL("../node_modules/shadcn/package.json", import.meta.url)),
);
const merge = requireFromShadcn("deepmerge");

test("deepmerge preserves own __proto__ data without poisoning the result", () => {
  for (const source of [{}, JSON.parse('{"__proto__":{"sourceMarker":true}}')]) {
    const target = JSON.parse('{"__proto__":{"targetMarker":true}}');
    const result = merge(target, source);
    assert.equal(Object.getPrototypeOf(result), Object.prototype);
    assert.equal(result.targetMarker, undefined);
    assert.equal(result.sourceMarker, undefined);
    assert.equal(Object.hasOwn(result, "__proto__"), true);
    assert.equal(result.__proto__.targetMarker, true);
  }
  assert.equal({}.targetMarker, undefined);
});

test("deepmerge retains normal nested, array, and custom merge behavior", () => {
  assert.deepEqual(
    merge({ nested: { left: 1 }, list: [1] }, { nested: { right: 2 }, list: [2] }),
    { nested: { left: 1, right: 2 }, list: [1, 2] },
  );
  assert.deepEqual(
    merge({ list: [1] }, { list: [2] }, { arrayMerge: (_left, right) => right }),
    { list: [2] },
  );
});

test("Mermaid's patched KaTeX still renders math without trusting unsafe links", () => {
  const requireFromMermaid = createRequire(
    realpathSync(new URL("../node_modules/mermaid/package.json", import.meta.url)),
  );
  const katex = requireFromMermaid("katex");
  const rendered = katex.renderToString("E = mc^2", { throwOnError: true, trust: false });
  assert.match(rendered, /<math/);
  assert.match(rendered, /katex/);
  const unsafe = katex.renderToString("\\href{javascript:alert(1)}{click}", { trust: false });
  assert.doesNotMatch(unsafe, /href=["']javascript:/i);
});

test("ts-morph source globs remain compatible with the minimatch override", () => {
  const { Project } = requireFromShadcn("ts-morph");
  const directory = mkdtempSync(path.join(tmpdir(), "axon-palette-glob-"));
  try {
    writeFileSync(path.join(directory, "included.ts"), "export const value = 1;\n");
    writeFileSync(path.join(directory, "excluded.test.ts"), "export {};\n");
    const project = new Project({ skipAddingFilesFromTsConfig: true });
    const files = project.addSourceFilesAtPaths([
      `${directory}/*.ts`,
      `!${directory}/*.test.ts`,
    ]);
    assert.deepEqual(files.map((file) => file.getBaseName()), ["included.ts"]);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
