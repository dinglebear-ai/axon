const test = require("node:test");
const assert = require("node:assert/strict");

const { buildContext, loadFiles } = require("./helpers/load-extension.js");

test("ask normalizer renders current canonical_uri citation objects", () => {
  const context = buildContext();
  loadFiles(context, [
    "src/sidepanel/launcher-data.js",
    "src/sidepanel/launcher-prep.js"
  ]);

  const result = context.AxonPrep.ask({
    answer: "Axon answer",
    citations: [{ canonical_uri: "session://codex/doc_123" }]
  });

  assert.equal(result.citations[0].src, "session://codex/doc_123");
});
