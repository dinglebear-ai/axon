"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { buildContext, loadFiles, EXT_ROOT } = require("./helpers/load-extension");

function mockFetch(calls, response) {
  return async (url, init = {}) => {
    calls.push({ url, method: init.method || "GET", body: init.body ? JSON.parse(init.body) : undefined });
    return {
      ok: true,
      status: 200,
      statusText: "OK",
      json: async () => response,
      text: async () => JSON.stringify(response)
    };
  };
}

test("scrapeWithAxon submits a POST /v1/sources page-scope request", async () => {
  const calls = [];
  const ctx = buildContext({ fetch: mockFetch(calls, { canonical_uri: "https://example.com" }) });
  loadFiles(ctx, [
    "src/redaction/capture-redaction.js",
    "src/popup/popup-state.js",
    "src/popup/popup-actions.js",
    "src/popup/popup-api.js",
    "src/popup/popup-format.js"
  ]);

  await ctx.scrapeWithAxon(["https://example.com"]);

  assert.equal(calls.length, 1);
  assert.equal(calls[0].method, "POST");
  assert.match(calls[0].url, /\/v1\/sources$/);
  assert.deepEqual(calls[0].body, {
    source: "https://example.com",
    scope: "page",
    execution: { mode: "foreground", priority: "normal", detached: false, heartbeat_interval_secs: 5 }
  });
});

test("startCrawlWithAxon submits a POST /v1/sources site-scope request per URL", async () => {
  const calls = [];
  const ctx = buildContext({ fetch: mockFetch(calls, { job_id: "job-1" }) });
  loadFiles(ctx, [
    "src/redaction/capture-redaction.js",
    "src/popup/popup-state.js",
    "src/popup/popup-actions.js",
    "src/popup/popup-api.js",
    "src/popup/popup-format.js"
  ]);

  await ctx.startCrawlWithAxon(["https://example.com"], {});

  assert.equal(calls.length, 1);
  assert.match(calls[0].url, /\/v1\/sources$/);
  assert.equal(calls[0].body.source, "https://example.com");
  assert.equal(calls[0].body.scope, "site");
});

test("rememberWithAxon submits a POST /v1/memories request matching RestMemoryRequest shape", async () => {
  const calls = [];
  const ctx = buildContext({ fetch: mockFetch(calls, { memory_id: "mem-1" }) });
  loadFiles(ctx, [
    "src/redaction/capture-redaction.js",
    "src/popup/popup-state.js",
    "src/popup/popup-actions.js",
    "src/popup/popup-api.js",
    "src/popup/popup-format.js"
  ]);

  const tab = { url: "https://example.com/page", title: "Example Page" };
  await ctx.rememberWithAxon(["decision", "ship", "it"], tab);

  assert.equal(calls.length, 1);
  assert.equal(calls[0].method, "POST");
  assert.match(calls[0].url, /\/v1\/memories$/);

  const allowedKeys = new Set(["memory_type", "body", "title"]);
  for (const key of Object.keys(calls[0].body)) {
    assert.ok(allowedKeys.has(key), `unexpected key "${key}" would 400 against RestMemoryRequest (deny_unknown_fields)`);
  }
  assert.equal(calls[0].body.memory_type, "decision");
  assert.equal(calls[0].body.body, "ship it");
});

test("rememberWithAxon refuses to capture a blocked-scheme tab URL", async () => {
  const calls = [];
  const ctx = buildContext({ fetch: mockFetch(calls, {}) });
  loadFiles(ctx, [
    "src/redaction/capture-redaction.js",
    "src/popup/popup-state.js",
    "src/popup/popup-actions.js",
    "src/popup/popup-api.js",
    "src/popup/popup-format.js"
  ]);

  const tab = { url: "chrome://extensions", title: "Extensions" };
  await assert.rejects(() => ctx.rememberWithAxon([], tab), /can't capture/);
  assert.equal(calls.length, 0);
});

// Regression lock for commit 7f4daa05d: the legacy per-action routes were
// folded into POST /v1/sources and must never reappear as a *live* route
// reference in any shipped extension script. Comments are stripped first —
// popup-api.js/launcher.js intentionally document the removed routes in
// prose (e.g. "the removed `/v1/crawl` route accepted...") to explain why
// /v1/sources is used instead, and that history is not itself a regression.
function stripComments(source) {
  return source.replace(/\/\*[\s\S]*?\*\//g, " ").replace(/(^|[^:])\/\/.*$/gm, "$1");
}

test("removed legacy routes never appear as a live reference in any shipped extension script", () => {
  const forbidden = [
    /\/v1\/scrape\b/,
    /\/v1\/crawl\b/,
    /\/v1\/embed\b/,
    /\/v1\/ingest\b/,
    /\/v1\/dedupe\b/,
    /\/v1\/purge\b/,
    /\/v1\/map\b/,
    /\/v1\/summarize\b/,
    /\/v1\/evaluate\b/,
    /\/v1\/suggest\b/,
    /\/v1\/extract\b/
  ];
  const jsFiles = listShippedJsFiles(path.join(EXT_ROOT, "src"));

  assert.ok(jsFiles.length > 0, "expected to find shipped .js files under apps/chrome-extension/src");

  for (const file of jsFiles) {
    const code = stripComments(fs.readFileSync(path.join(EXT_ROOT, "src", file), "utf8"));
    for (const pattern of forbidden) {
      assert.doesNotMatch(code, pattern, `src/${file} must not reference the removed route ${pattern}`);
    }
  }
});

// Recursively lists .js files under `srcRoot`, returned as paths relative to
// `srcRoot` (posix-style, since these are used to build require/read paths).
function listShippedJsFiles(srcRoot, prefix = "") {
  const entries = fs.readdirSync(path.join(srcRoot, prefix), { withFileTypes: true });
  const out = [];
  for (const entry of entries) {
    const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) {
      out.push(...listShippedJsFiles(srcRoot, rel));
    } else if (entry.isFile() && entry.name.endsWith(".js")) {
      out.push(rel);
    }
  }
  return out;
}

test("popup.html only wires the current source-request pipeline (sanity check for the loader's file list)", () => {
  const html = fs.readFileSync(path.join(EXT_ROOT, "src", "popup", "popup.html"), "utf8");
  assert.match(html, /popup-api\.js/);
  assert.match(html, /capture-redaction\.js/);
});
