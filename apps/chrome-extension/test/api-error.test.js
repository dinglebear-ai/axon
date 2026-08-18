const test = require("node:test");
const assert = require("node:assert/strict");

const { AxonApiError } = require("../src/auth/api-error.js");

test("formats the current structured Axon error envelope", () => {
  const text = JSON.stringify({
    error: {
      code: "provider.unavailable",
      message: "web source indexing failed"
    }
  });

  assert.equal(
    AxonApiError.messageFromResponseText(text, "HTTP 502"),
    "web source indexing failed (provider.unavailable)"
  );
});

test("preserves plain-text server errors", () => {
  assert.equal(AxonApiError.messageFromResponseText("gateway unavailable", "HTTP 502"), "gateway unavailable");
});
