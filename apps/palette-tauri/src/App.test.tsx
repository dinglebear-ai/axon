// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import App from "./App";
import { invoke } from "./lib/invoke";

vi.mock("./lib/invoke", async () => {
  const actual = await vi.importActual<typeof import("./lib/invoke")>("./lib/invoke");
  return {
    ...actual,
    invoke: vi.fn(),
  };
});

const config = {
  serverUrl: "http://127.0.0.1:9999",
  token: null,
  shortcut: "Ctrl+Space",
  collection: "axon",
  resultLimit: 10,
  theme: "dark",
  hideOnBlur: false,
  openResultsInline: true,
  envValues: {},
  configValues: {},
};

describe("App local help", () => {
  afterEach(() => cleanup());

  beforeEach(() => {
    Object.defineProperty(window, "matchMedia", {
      writable: true,
      value: vi.fn().mockImplementation((query: string) => ({
        matches: false,
        media: query,
        onchange: null,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
    });
    HTMLElement.prototype.scrollIntoView = vi.fn();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "load_palette_config" || command === "load_palette_default_config") return config;
      if (command === "resize_palette" || command === "hide_palette") return undefined;
      if (command === "axon_http_request") throw new Error("REST should not be called for local help");
      return undefined;
    });
  });

  async function renderAndType(value: string) {
    render(<App />);

    const input = await screen.findByLabelText("Axon command");
    fireEvent.change(input, { target: { value } });
    return input;
  }

  it.each(["help scrape", "scrape help"])("runs %s from Enter as local help", async (command) => {
    const input = await renderAndType(command);
    fireEvent.keyDown(input, { key: "Enter" });

    expect((await screen.findAllByText("POST /v1/scrape")).length).toBeGreaterThan(0);
    expect(screen.getAllByText("Scrape URL").length).toBeGreaterThan(0);
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith("axon_http_request", expect.anything());
  });

  it("runs question-mark catalog help from Enter without REST", async () => {
    const input = await renderAndType("?");
    fireEvent.keyDown(input, { key: "Enter" });

    expect(await screen.findByRole("heading", { name: "Axon Palette Help" })).toBeTruthy();
    expect(screen.getAllByText("scrape").length).toBeGreaterThan(0);
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith("axon_http_request", expect.anything());
  });

  it("opens selected action help from the command bar and replays it from history as local help", async () => {
    await renderAndType("scrape");
    const commandHelp = (await screen.findAllByLabelText("Help for Scrape URL")).find((button) =>
      button.classList.contains("command-help"),
    );
    expect(commandHelp).toBeDefined();
    fireEvent.click(commandHelp!);

    expect((await screen.findAllByText("Scrape URL")).length).toBeGreaterThan(0);
    expect(screen.getAllByText("POST /v1/scrape").length).toBeGreaterThan(0);
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith("axon_http_request", expect.anything());

    fireEvent.click(screen.getByText("↺ recent"));
    fireEvent.click(await screen.findByRole("button", { name: /scrape.*Help.*just now/i }));

    await waitFor(() => expect(screen.getAllByText("POST /v1/scrape").length).toBeGreaterThan(0));
    expect(screen.getAllByText("Scrape URL").length).toBeGreaterThan(0);
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith("axon_http_request", expect.anything());
  });

  it("shows unknown-query help from the command-bar question mark without REST", async () => {
    await renderAndType("nope");

    fireEvent.click(await screen.findByRole("button", { name: "Help" }));

    expect(await screen.findByText("No matching action:")).toBeTruthy();
    expect(screen.getByText("nope")).toBeTruthy();
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith("axon_http_request", expect.anything());
  });
});
