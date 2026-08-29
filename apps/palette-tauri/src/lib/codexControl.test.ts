import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Client } from "./axonClient";
import {
  approveCodexOperation,
  buildMcpConfigMutation,
  CODEX_MUTATIONS,
  prepareCodexOperation,
  readCodexSnapshot,
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
    expect(invoke.mock.calls[1][1].request.path).toContain("/approve");
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
      value: null,
    });
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
