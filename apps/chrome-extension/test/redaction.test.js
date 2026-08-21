"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { buildContext, loadFiles } = require("./helpers/load-extension");

function loadAxonRedact() {
  const ctx = buildContext();
  loadFiles(ctx, ["src/redaction/capture-redaction.js"]);
  return ctx.AxonRedact;
}

test("redactText masks a high-confidence bearer token", () => {
  const token = ["sk_", "live_abcdef0123456789abcdefghijkl"].join("");
  const { text, redactions } = loadAxonRedact().redactText(`Authorization header: Bearer ${token}`);
  assert.match(text, /\[REDACTED\]/);
  assert.doesNotMatch(text, new RegExp(token));
  assert.ok(redactions.length > 0);
});

test("redactText masks an Authorization header line", () => {
  const { text } = loadAxonRedact().redactText("Authorization: Bearer abcdefghijklmnopqrstuvwxyz012345");
  assert.match(text, /^Authorization:\s*Bearer\s+\[REDACTED\]$/m);
  assert.doesNotMatch(text, /abcdefghijklmnopqrstuvwxyz012345/);
});

test("redactText masks a credential-shaped Cookie header line", () => {
  const { text } = loadAxonRedact().redactText("Cookie: session=abc123def456ghi789jkl012; other=xyz789");
  assert.match(text, /Cookie:\s*\[REDACTED\]/);
});

test("redactText masks credentials embedded in a URL", () => {
  const { text } = loadAxonRedact().redactText("fetched https://user:hunter2pass@example.com/path");
  assert.match(text, /https:\/\/\[REDACTED\]@example\.com\/path/);
  assert.doesNotMatch(text, /hunter2pass/);
});

test("redactText masks a high-confidence secret key=value pair", () => {
  const token = ["sk_", "live_abcdef0123456789abcdefghijkl"].join("");
  const { text } = loadAxonRedact().redactText(`api_key: "${token}"`);
  assert.match(text, /api_key:\s*\[REDACTED\]/);
});

test("redactText preserves credential identifiers in captured document text", () => {
  const input = "AKIA0123456789ABCDEF is an AWS access-key-id example";
  const { text, redactions } = loadAxonRedact().redactText(input);
  assert.equal(text, input);
  assert.equal(redactions.length, 0);
});

test("redactText masks a JWT-shaped token", () => {
  const jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
  const { text } = loadAxonRedact().redactText(`token=${jwt}`);
  assert.match(text, /\[REDACTED(?:_JWT)?\]/);
  assert.doesNotMatch(text, /eyJhbGciOiJIUzI1NiJ9/);
});

test("redactText leaves ordinary non-secret prose untouched", () => {
  const input = "The quick brown fox jumps over the lazy dog. Visit https://example.com/docs for more info.";
  const { text, redactions } = loadAxonRedact().redactText(input);
  assert.equal(text, input);
  assert.equal(redactions.length, 0);
});

test("redactText leaves a short non-secret identifier untouched", () => {
  const input = "issue #4821 was fixed in build 2026.07.10";
  const { text, redactions } = loadAxonRedact().redactText(input);
  assert.equal(text, input);
  assert.equal(redactions.length, 0);
});

test("redactText preserves low-confidence documentation examples", () => {
  const input = [
    "Authorization: Bearer abc123",
    "Cookie: theme=dark; mode=compact",
    "postgres://user:password@localhost/app",
    "TOKEN=abc123"
  ].join("\n");
  const { text, redactions } = loadAxonRedact().redactText(input);
  assert.equal(text, input);
  assert.equal(redactions.length, 0);
});

test("redactText preserves long hashes and identifiers", () => {
  const hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
  const id = "artifact0123456789abcdef0123456789abcdef";
  const input = `sha256=${hash} id=${id}`;
  const { text, redactions } = loadAxonRedact().redactText(input);
  assert.equal(text, input);
  assert.equal(redactions.length, 0);
});

test("redactUrl redacts credential query values but preserves benign metadata", () => {
  const url = loadAxonRedact().redactUrl(
    "https://example.com/search?q=rust&pageToken=next-42&tokenCount=4&accessToken=secret&X-Amz-Date=20260819T120000Z&X-Amz-Signature=abc"
  );
  const parsed = new URL(url);
  assert.equal(parsed.searchParams.get("q"), "rust");
  assert.equal(parsed.searchParams.get("pageToken"), "next-42");
  assert.equal(parsed.searchParams.get("tokenCount"), "4");
  assert.equal(parsed.searchParams.get("X-Amz-Date"), "20260819T120000Z");
  assert.equal(parsed.searchParams.get("accessToken"), "REDACTED");
  assert.equal(parsed.searchParams.get("X-Amz-Signature"), "REDACTED");
});

test("redactUrl strips basic-auth credentials", () => {
  const url = loadAxonRedact().redactUrl("https://admin:s3cr3t@internal.example.com/dash");
  assert.equal(url, "https://[REDACTED]@internal.example.com/dash");
});

test("redactUrl leaves a plain URL untouched", () => {
  const url = loadAxonRedact().redactUrl("https://example.com/page?q=1");
  assert.equal(url, "https://example.com/page?q=1");
});
