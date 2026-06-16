// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { axe } from "jest-axe";
import { afterEach, describe, expect, it, vi } from "vitest";

import { PaletteCommandBar } from "@/components/palette/PaletteCommandBar";
import { ACTIONS, type PaletteAction } from "@/lib/actions";

const config = {
  serverUrl: "http://127.0.0.1:9999",
  token: null,
  shortcut: "Ctrl+Space",
  collection: "axon",
  resultLimit: 10,
  theme: "dark" as const,
  hideOnBlur: false,
  openResultsInline: true,
  envValues: {},
  configValues: {},
};

const scrape = ACTIONS.find((a) => a.subcommand === "scrape") as PaletteAction;

function renderBar(overrides: Partial<Parameters<typeof PaletteCommandBar>[0]> = {}) {
  const props = {
    active: scrape,
    activeDescendantId: "action-scrape",
    config,
    endpointLabel: "127.0.0.1:9999",
    endpointTone: "syncing",
    hasQuery: true,
    listboxOpen: true,
    modeAction: null,
    query: "scrape",
    running: false,
    settingsOpen: false,
    showBackButton: false,
    submitDisabled: false,
    validation: "",
    onBack: vi.fn(),
    onHelp: vi.fn(),
    onInputKeyDown: vi.fn(),
    onQueryChange: vi.fn(),
    onReset: vi.fn(),
    onSubmit: vi.fn(),
    onSwitchAction: vi.fn(),
    onToggleMaximize: vi.fn(),
    onToggleSettings: vi.fn(),
    ...overrides,
  };
  return { props, ...render(<PaletteCommandBar {...props} />) };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("PaletteCommandBar combobox ARIA (A11Y-C1)", () => {
  it("exposes the input as a combobox wired to the listbox", () => {
    renderBar();
    const input = screen.getByRole("combobox");
    expect(input).toHaveAttribute("aria-expanded", "true");
    expect(input).toHaveAttribute("aria-controls", "palette-action-list");
    expect(input).toHaveAttribute("aria-autocomplete", "list");
    expect(input).toHaveAttribute("aria-activedescendant", "action-scrape");
  });

  it("drops aria-expanded and active descendant when the listbox is closed", () => {
    renderBar({ listboxOpen: false });
    const input = screen.getByRole("combobox");
    expect(input).toHaveAttribute("aria-expanded", "false");
    expect(input).not.toHaveAttribute("aria-activedescendant");
  });

  it("ties validation text to the input via aria-describedby (A11Y-M2)", () => {
    renderBar({ validation: "Enter a URL" });
    const input = screen.getByRole("combobox");
    expect(input).toHaveAttribute("aria-describedby", "command-validation");
    expect(screen.getByText("Enter a URL")).toHaveAttribute("id", "command-validation");
  });

  it("has no axe violations (T-C1)", async () => {
    // Render collapsed so the combobox carries no dangling IDREFs (the listbox
    // and active option live in ActionList, not the command bar).
    const { container } = renderBar({ listboxOpen: false });
    expect(await axe(container)).toHaveNoViolations();
  });
});

describe("PaletteCommandBar action switcher disclosure (A11Y-H1 / T-M4)", () => {
  const ask = ACTIONS.find((a) => a.subcommand === "ask") as PaletteAction;

  it("is a closed disclosure (not a role=menu) until activated", () => {
    renderBar({ modeAction: scrape });
    const trigger = screen.getByRole("button", { name: /Switch from/ });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveAttribute("aria-haspopup", "true");
    // No ARIA menu contract is advertised.
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("opens to plain Tab-focusable buttons and switches action on click", async () => {
    const user = userEvent.setup();
    const onSwitchAction = vi.fn();
    renderBar({ modeAction: scrape, onSwitchAction });

    await user.click(screen.getByRole("button", { name: /Switch from/ }));
    expect(screen.getByRole("button", { name: /Switch from/ })).toHaveAttribute("aria-expanded", "true");

    const askButton = screen
      .getAllByRole("button")
      .find((b) => b.textContent?.includes(ask.subcommand));
    expect(askButton).toBeDefined();
    await user.click(askButton!);
    expect(onSwitchAction).toHaveBeenCalledTimes(1);
    expect(onSwitchAction.mock.calls[0][0].subcommand).toBe("ask");
  });

  it("closes the disclosure on Escape and keeps focus on the trigger", async () => {
    const user = userEvent.setup();
    renderBar({ modeAction: scrape });
    const trigger = screen.getByRole("button", { name: /Switch from/ });
    await user.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");

    trigger.focus();
    await user.keyboard("{Escape}");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveFocus();
  });
});
