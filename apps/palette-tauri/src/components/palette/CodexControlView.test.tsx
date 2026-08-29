import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Client } from "@/lib/axonClient";
import { CodexControlView, mutationParams } from "./CodexControlView";

vi.mock("@/lib/useCodexControl", () => ({
  useCodexControl: () => ({
    snapshot: null,
    events: [],
    operations: [],
    error: null,
    loading: false,
    refresh: vi.fn(),
    dismissEvent: vi.fn(),
  }),
}));

const client = { baseUrl: "https://axon.example", headers: {} } as Client;

describe("CodexControlView mutation validation", () => {
  it("exposes account and provider context reads", async () => {
    const user = userEvent.setup();
    render(<CodexControlView client={client} onClose={vi.fn()} />);

    const reads = screen.getByLabelText("Read action");
    await user.selectOptions(reads, "rate_limits_read");
    expect(reads).toHaveValue("rate_limits_read");
    await user.selectOptions(reads, "model_provider_capabilities_read");
    expect(reads).toHaveValue("model_provider_capabilities_read");
  });

  it("shows typed JSON parse failures inline", async () => {
    const user = userEvent.setup();
    render(<CodexControlView client={client} onClose={vi.fn()} />);

    await user.selectOptions(screen.getByLabelText("Workflow"), "config");
    await user.type(screen.getByLabelText("Target"), "model");
    await user.type(screen.getByLabelText("Value (JSON)"), "gpt-test");

    expect(screen.getByRole("alert")).toHaveTextContent("Config value must be valid JSON");
    expect(screen.getByRole("button", { name: "1 Prepare" })).toBeDisabled();
  });

  it("surfaces MCP transport validation instead of swallowing it", async () => {
    const user = userEvent.setup();
    render(<CodexControlView client={client} onClose={vi.fn()} />);

    await user.selectOptions(screen.getByLabelText("Workflow"), "mcpConfig");
    await user.type(screen.getByLabelText("Target"), "local");
    await user.type(screen.getByLabelText("Command"), "node server.js");

    expect(screen.getByRole("alert")).toHaveTextContent("Command must be one executable");
  });

  it("enables Prepare for a validated multi-write batch", async () => {
    const user = userEvent.setup();
    render(<CodexControlView client={client} onClose={vi.fn()} />);

    await user.selectOptions(screen.getByLabelText("Workflow"), "configBatch");
    fireEvent.change(screen.getByLabelText("Batch writes (JSON array or object)"), {
      target: {
        value:
          '[{"keyPath":"model","mergeStrategy":"upsert","value":"gpt-test"},{"keyPath":"features.fast","mergeStrategy":"replace","value":true}]',
      },
    });

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "1 Prepare" })).toBeEnabled();
  });
});

describe("Codex 0.150 mutation payloads", () => {
  it("uses only schema-native MCP OAuth fields", () => {
    expect(mutationParams("mcpOauth", "github", "", "")).toEqual({
      name: "github",
    });
    expect(
      mutationParams("mcpOauth", "github", '{"scopes":["read"],"timeoutSecs":30}', ""),
    ).toEqual({ name: "github", scopes: ["read"], timeoutSecs: 30 });
    expect(() => mutationParams("mcpOauth", "github", '{"provider":"legacy"}', "")).toThrow(
      "Unsupported MCP OAuth option: provider",
    );
  });

  it("does not invent provenance fields for native plugin and marketplace requests", () => {
    expect(mutationParams("pluginInstall", "demo", "", "https://ignored.example")).toEqual({
      pluginName: "demo",
    });
    expect(mutationParams("marketplaceAdd", "", "", "https://market.example")).toEqual({
      source: "https://market.example",
    });
    expect(mutationParams("skillImport", "", "[]", "")).toEqual({
      migrationItems: [],
    });
  });
});
