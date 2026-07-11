"use strict";

// Minimal loader for the extension's classic (non-module) scripts under a
// node:vm context, so unit tests can exercise the real production code
// instead of re-implementing its logic in the test file.
//
// The extension's popup/background scripts are plain <script>-concatenated
// globals (see popup.html load order), not ES modules or CommonJS — so
// there is nothing to `require()`. We read each file's source and run it
// in a shared vm context that stubs the browser globals (`chrome`,
// `document`, `fetch`, `navigator`) each file touches at parse time.

const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const EXT_ROOT = path.join(__dirname, "..", "..");

function fakeElement() {
  const el = {
    addEventListener() {},
    removeEventListener() {},
    setAttribute() {},
    getAttribute() {
      return null;
    },
    classList: { add() {}, remove() {}, toggle() {}, contains() { return false; } },
    style: {},
    dataset: {},
    appendChild() {},
    querySelector() {
      return null;
    },
    querySelectorAll() {
      return [];
    },
    get textContent() {
      return this._text || "";
    },
    set textContent(v) {
      this._text = v;
    },
    value: ""
  };
  return el;
}

function fakeDocument() {
  return {
    querySelector() {
      return fakeElement();
    },
    querySelectorAll() {
      return [];
    },
    createElement() {
      return fakeElement();
    },
    addEventListener() {},
    body: fakeElement()
  };
}

function defaultChromeMock(storage) {
  const store = storage || {};
  return {
    storage: {
      local: {
        async get(keys) {
          if (!keys) return { ...store };
          const list = Array.isArray(keys) ? keys : [keys];
          const out = {};
          for (const k of list) out[k] = store[k];
          return out;
        },
        async set(values) {
          Object.assign(store, values);
        }
      },
      onChanged: { addListener() {} }
    },
    tabs: {
      async query() {
        return [];
      },
      onActivated: { addListener() {} },
      onUpdated: { addListener() {} }
    },
    runtime: {
      onMessage: { addListener() {} },
      getURL: (p) => `chrome-extension://axon/${p}`
    },
    contextMenus: { create() {}, onClicked: { addListener() {} } },
    permissions: {
      async contains() {
        return true;
      },
      async request() {
        return true;
      }
    }
  };
}

// Builds a fresh vm context ("window") with browser globals stubbed.
function buildContext(overrides = {}) {
  const sandbox = {
    console,
    setTimeout,
    clearTimeout,
    URL,
    URLSearchParams,
    fetch: overrides.fetch,
    chrome: overrides.chrome || defaultChromeMock(),
    document: overrides.document || fakeDocument(),
    navigator: overrides.navigator || { clipboard: { async writeText() {} } },
    loadConfig:
      overrides.loadConfig ||
      (async () => ({ axonUrl: "http://axon.test", axonToken: "test-token" }))
  };
  sandbox.window = sandbox;
  sandbox.self = sandbox;
  sandbox.globalThis = sandbox;
  vm.createContext(sandbox);
  return sandbox;
}

// Runs a list of extension source files (relative to apps/chrome-extension)
// in order inside the given context, mimicking popup.html's <script> order.
function loadFiles(context, files) {
  for (const file of files) {
    const fullPath = path.join(EXT_ROOT, file);
    const code = fs.readFileSync(fullPath, "utf8");
    vm.runInContext(code, context, { filename: fullPath });
  }
  return context;
}

module.exports = { buildContext, loadFiles, EXT_ROOT };
