import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Client } from "./axonClient";
import {
  approveCodexOperation,
  buildConfigBatchMutation,
  buildMcpConfigMutation,
  CODEX_MUTATIONS,
  executeCodexOperation,
  parseConfigValue,
  prepareCodexOperation,
  readCodexSnapshot,
  reconcileCodexOperation,
} from "./codexControl";

const invoke = vi.fn();
vi.mock("./invoke", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const client: Client = {
  baseUrl: "https://axon.example",
  headers: { Authorization: "Bearer redacted" },
};

beforeEach(() => invoke.mockReset());

describe("Codex control client", () => {
  it("reads the server-host snapshot through the shared bridge", async () => {
    invoke.mockResolvedValue({ ok: true, status: 200, payload: { status: { state: "ready" } } });
    await expect(readCodexSnapshot(client)).resolves.toMatchObject({ status: { state: "ready" } });
    expect(invoke.mock.calls[0][1].request.path).toBe("/v1/codex");
  });

  it("uses prepare and separate approval requests", async () => {
    invoke
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        payload: { id: 7, phase: "pending", request_digest: "abc" },
      })
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        payload: { approval_capability: "single-use" },
      });
    const operation = await prepareCodexOperation(client, {
      ...CODEX_MUTATIONS.pluginInstall,
      params: { plugin: "demo" },
    });
    expect(operation.id).toBe(7);
    await expect(approveCodexOperation(client, 7)).resolves.toBe("single-use");
    expect(invoke.mock.calls[0][1].request.body).toEqual({
      action: "plugin_install",
      idempotency_key: expect.any(String),
      redacted_request: { plugin: "demo" },
    });
    expect(invoke.mock.calls[1][1].request.path).toContain("/approve");
    expect(invoke.mock.calls[1][1].request.body).toEqual({});
  });

  it("executes the exact typed mutation without client revision state", async () => {
    invoke.mockResolvedValue({ ok: true, status: 200, payload: { result: { ok: true } } });
    await executeCodexOperation(client, 7, "single-use", {
      ...CODEX_MUTATIONS.config,
      params: { keyPath: "model", value: "gpt-test" },
    });
    expect(invoke.mock.calls[0][1].request.body).toEqual({
      capability: "single-use",
      action: "config_value_write",
      params: { keyPath: "model", value: "gpt-test" },
    });
  });

  it("reconciles recovery with an empty body", async () => {
    invoke.mockResolvedValue({ ok: true, status: 200, payload: {} });
    await reconcileCodexOperation(client, 9);
    expect(invoke.mock.calls[0][1].request).toMatchObject({
      path: "/v1/codex/operations/9/reconcile",
      body: {},
    });
  });
});

describe("config mutation inputs", () => {
  it("parses config values as typed JSON", () => {
    expect(parseConfigValue("true")).toBe(true);
    expect(parseConfigValue("42")).toBe(42);
    expect(parseConfigValue('"gpt-test"')).toBe("gpt-test");
    expect(parseConfigValue('{"nested": [1]}')).toEqual({ nested: [1] });
    expect(() => parseConfigValue("gpt-test")).toThrow("valid JSON");
  });

  it("builds a genuine multi-write config batch from an array", () => {
    expect(
      buildConfigBatchMutation(
        '[{"keyPath":"model","mergeStrategy":"upsert","value":"gpt-test"},{"keyPath":"features.fast","mergeStrategy":"replace","value":true}]',
      ),
    ).toEqual({
      edits: [
        { keyPath: "model", mergeStrategy: "upsert", value: "gpt-test" },
        { keyPath: "features.fast", mergeStrategy: "replace", value: true },
      ],
    });
  });

  it("requires the native edits shape and validates every write", () => {
    expect(() => buildConfigBatchMutation('{"writes":[]}')).toThrow("edits array");
    expect(() =>
      buildConfigBatchMutation('[{"keyPath":"model","mergeStrategy":"upsert","value":"only-one"}]'),
    ).toThrow("at least two writes");
    expect(() =>
      buildConfigBatchMutation(
        '[{"keyPath":"model","mergeStrategy":"upsert","value":"ok"},{"keyPath":"","mergeStrategy":"replace","value":false}]',
      ),
    ).toThrow("non-empty keyPath");
  });
});

describe("MCP config mutations", () => {
  it("builds a validated stdio definition with argument and secret references", () => {
    expect(
      buildMcpConfigMutation({
        name: "local",
        command: "node",
        args: '["server.js", "--stdio"]',
        url: "",
        env: "TOKEN=env:MY_TOKEN",
        remove: false,
      }),
    ).toEqual({
      keyPath: "mcp_servers.local",
      mergeStrategy: "upsert",
      value: { command: "node", args: ["server.js", "--stdio"], env: { TOKEN: "env:MY_TOKEN" } },
    });
  });

  it("builds URL definitions and explicit removal", () => {
    expect(
      buildMcpConfigMutation({
        name: "remote",
        command: "",
        args: "",
        url: "https://mcp.example.test",
        env: "",
        remove: false,
      }),
    ).toEqual({
      keyPath: "mcp_servers.remote",
      mergeStrategy: "upsert",
      value: { url: "https://mcp.example.test" },
    });
    expect(
      buildMcpConfigMutation({
        name: "remote",
        command: "",
        args: "",
        url: "",
        env: "",
        remove: true,
      }),
    ).toEqual({
      keyPath: "mcp_servers.remote",
      mergeStrategy: "replace",
      value: null,
    });
  });

  it("builds the exact MCP recovery mutation request", () => {
    const params = buildMcpConfigMutation({
      name: "recovered",
      command: "codex",
      args: '["mcp", "serve"]',
      url: "",
      env: "TOKEN=env:CODEX_TOKEN",
      remove: false,
    });
    expect({ ...CODEX_MUTATIONS.config, params }).toEqual({
      action: "config_value_write",
      params: {
        keyPath: "mcp_servers.recovered",
        mergeStrategy: "upsert",
        value: {
          command: "codex",
          args: ["mcp", "serve"],
          env: { TOKEN: "env:CODEX_TOKEN" },
        },
      },
    });
  });

  it("rejects command-only fields for URL MCP transports", () => {
    expect(() =>
      buildMcpConfigMutation({
        name: "remote",
        command: "",
        args: '["--verbose"]',
        url: "https://mcp.example.test",
        env: "",
        remove: false,
      }),
    ).toThrow("URL MCP transports do not accept command arguments");
    expect(() =>
      buildMcpConfigMutation({
        name: "remote",
        command: "",
        args: "[]",
        url: "https://mcp.example.test",
        env: "TOKEN=env:MCP_TOKEN",
        remove: false,
      }),
    ).toThrow("URL MCP transports do not accept environment entries");
  });

  it("rejects mixed transports, shell commands, invalid args, and plaintext secrets", () => {
    expect(() =>
      buildMcpConfigMutation({
        name: "x",
        command: "node server.js",
        args: "[]",
        url: "",
        env: "",
        remove: false,
      }),
    ).toThrow("executable");
    expect(() =>
      buildMcpConfigMutation({
        name: "x",
        command: "node",
        args: "{}",
        url: "",
        env: "",
        remove: false,
      }),
    ).toThrow("JSON array");
    expect(() =>
      buildMcpConfigMutation({
        name: "x",
        command: "node",
        args: "[]",
        url: "https://example.test",
        env: "",
        remove: false,
      }),
    ).toThrow("either command or URL");
    expect(() =>
      buildMcpConfigMutation({
        name: "x",
        command: "node",
        args: "[]",
        url: "",
        env: "TOKEN=plaintext",
        remove: false,
      }),
    ).toThrow("env: secret reference");
  });
});
