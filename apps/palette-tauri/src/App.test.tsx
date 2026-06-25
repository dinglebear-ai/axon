// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { axe } from "jest-axe";
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

describe("App command palette accessibility + keyboard nav", () => {
  afterEach(() => cleanup());

  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "load_palette_config" || command === "load_palette_default_config") return config;
      return undefined;
    });
  });

  async function renderApp() {
    render(<App />);
    return await screen.findByRole("combobox");
  }

  // T-C1 — axe on the rendered command palette. On first mount the listbox is not
  // shown, so the combobox is collapsed (aria-expanded=false) and carries no
  // aria-controls/aria-activedescendant dangling IDREFs. Then open the listbox via
  // ArrowDown and re-run axe so the expanded state (with real referenced ids) is
  // also clean.
  it("renders the input as a combobox with no axe violations", async () => {
    const user = userEvent.setup();
    const { container } = render(<App />);
    const input = await screen.findByRole("combobox");
    expect(input).toHaveAttribute("aria-autocomplete", "list");
    expect(input).toHaveAttribute("aria-expanded", "false");
    expect(input).not.toHaveAttribute("aria-controls");
    expect(await axe(container)).toHaveNoViolations();

    input.focus();
    await user.keyboard("{ArrowDown}");
    await screen.findByRole("listbox", { name: "Actions" });
    expect(input).toHaveAttribute("aria-expanded", "true");
    expect(input).toHaveAttribute("aria-controls", "palette-action-list");
    expect(await axe(container)).toHaveNoViolations();
  });

  // T-H1 — ArrowDown opens the listbox and tracks the active descendant.
  it("opens the listbox on ArrowDown and exposes a role=listbox of options", async () => {
    const user = userEvent.setup();
    const input = await renderApp();
    input.focus();
    await user.keyboard("{ArrowDown}");

    const listbox = await screen.findByRole("listbox", { name: "Actions" });
    expect(listbox).toBeInTheDocument();
    const options = screen.getAllByRole("option");
    expect(options.length).toBeGreaterThan(0);
    // The combobox points at one of the options as the active descendant.
    const active = input.getAttribute("aria-activedescendant");
    expect(active).toMatch(/^action-/);
    expect(options.some((o) => o.id === active)).toBe(true);
  });

  // T-H1 — Enter on a partial (non-invoked) match for an arg-taking action enters
  // argument mode rather than submitting. "scr" matches scrape by fuzzy search but
  // is not the exact invoked subcommand, so Enter routes to enterActionMode and the
  // switcher disclosure (which only renders in mode) appears. No REST submit fires.
  it("enters argument mode on Enter for an action that needs an argument", async () => {
    const user = userEvent.setup();
    const input = await renderApp();
    await user.type(input, "scr");
    await user.keyboard("{Enter}");

    expect(await screen.findByRole("button", { name: /Switch from/ })).toBeInTheDocument();
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith("axon_http_request", expect.anything());
  });

  it("auto-runs only safe no-input switcher actions", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "load_palette_config" || command === "load_palette_default_config") return config;
      if (command === "axon_http_request") {
        return { ok: true, status: 200, method: "GET", path: "/v1/sources", payload: { sources: [] } };
      }
      return undefined;
    });
    const user = userEvent.setup();
    const input = await renderApp();
    await user.type(input, "scr");
    await user.keyboard("{Enter}");

    await user.click(await screen.findByRole("button", { name: /Switch from/ }));
    await user.click(await screen.findByRole("button", { name: /Sources/ }));

    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "axon_http_request",
        expect.objectContaining({
          request: expect.objectContaining({ path: "/v1/sources", method: "GET" }),
        }),
      ),
    );

    fireEvent.click(await screen.findByRole("button", { name: /Switch from/ }));
    vi.mocked(invoke).mockClear();
    fireEvent.click(await screen.findByRole("button", { name: /Dedupe/ }));

    expect(await screen.findByRole("button", { name: /Switch from Dedupe collection/ })).toBeInTheDocument();
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith("axon_http_request", expect.anything());
  });

  it("requires a second Enter before running guarded actions", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "load_palette_config" || command === "load_palette_default_config") return config;
      if (command === "axon_http_request") {
        return {
          ok: true,
          status: 200,
          method: "POST",
          path: "/v1/dedupe",
          payload: { collection: "axon", scanned: 10, removed: 0 },
        };
      }
      return undefined;
    });
    const user = userEvent.setup();
    const input = await renderApp();
    await user.type(input, "dedupe");
    await user.keyboard("{Enter}");

    expect(await screen.findByText(/confirmation armed/i)).toBeInTheDocument();
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith("axon_http_request", expect.anything());

    await user.keyboard("{Enter}");

    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "axon_http_request",
        expect.objectContaining({
          request: expect.objectContaining({ path: "/v1/dedupe", method: "POST" }),
        }),
      ),
    );
  });

  // T-H1 — Escape backs out of argument mode rather than submitting.
  it("clears argument mode on Escape", async () => {
    const user = userEvent.setup();
    const input = await renderApp();
    await user.type(input, "scr");
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("button", { name: /Switch from/ })).toBeInTheDocument();

    await user.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /Switch from/ })).toBeNull(),
    );
  });
});
