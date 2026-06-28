// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, expect, it, vi } from "vitest";

import { ActionList } from "@/components/palette/ActionList";
import { ACTIONS, type PaletteAction } from "@/lib/actions";
import { parseCommand } from "@/lib/paletteView";

const onSubmit = vi.fn();
const onEnterMode = vi.fn();
const onHelp = vi.fn();

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function Harness() {
  const [selected, setSelected] = useState(0);
  return (
    <ActionList
      filtered={ACTIONS.slice(0, 3)}
      selected={selected}
      setSelected={setSelected}
      parsed={parseCommand("")}
      onSubmit={onSubmit}
      onEnterMode={onEnterMode}
      onHelp={onHelp}
    />
  );
}

// The per-row help control is a pointer affordance kept out of the listbox a11y
// tree (its `.action-meta` wrapper is aria-hidden), so role queries must opt into
// hidden elements; its onClick still fires on mouse activation.
it("opens selected-row help without submitting or entering action mode", () => {
  render(<Harness />);
  fireEvent.click(screen.getByRole("button", { name: "Help for Help", hidden: true }));
  expect(onHelp).toHaveBeenCalledTimes(1);
  expect(onSubmit).not.toHaveBeenCalled();
  expect(onEnterMode).not.toHaveBeenCalled();
});

it("reveals row help on hover before running the row action", () => {
  render(<Harness />);

  const scrapeRow = screen.getByText((text, node) => (
    text === "Scrape" &&
    node instanceof HTMLElement &&
    node.classList.contains("action-label")
  )).closest(".action-row");
  if (!scrapeRow) throw new Error("missing Scrape URL row");
  fireEvent.pointerEnter(scrapeRow);
  fireEvent.click(screen.getByRole("button", { name: "Help for Scrape URL", hidden: true }));

  expect(onHelp).toHaveBeenCalledTimes(1);
  expect(onHelp.mock.calls[0]?.[0].subcommand).toBe("scrape");
  expect(onSubmit).not.toHaveBeenCalled();
  expect(onEnterMode).not.toHaveBeenCalled();
});

// A11Y-C1 — listbox/option semantics + stable option ids. Section headings and
// secondary row controls are kept OUT of the listbox a11y tree (presentation +
// aria-hidden) so the listbox's only meaningful children are role=option.
it("exposes a labelled listbox of options with stable ids", () => {
  render(<Harness />);
  const listbox = screen.getByRole("listbox", { name: "Actions" });
  expect(listbox).toHaveAttribute("id", "palette-action-list");
  const options = screen.getAllByRole("option");
  expect(options.length).toBe(3);
  // The first option is selected and carries the contract-stable id.
  expect(options[0]).toHaveAttribute("aria-selected", "true");
  expect(options[0].id).toMatch(/^action-/);
  // The category heading is hidden from AT (option labels carry the meaning).
  expect(listbox.querySelector(".action-section-heading")).toHaveAttribute("aria-hidden", "true");
});

// T-L3 — userEvent click enters argument mode for an arg-taking action.
it("enters argument mode on row click for an action that needs input", async () => {
  const user = userEvent.setup();
  render(<Harness />);

  const scrapeLabel = screen.getByText(
    (text, node) =>
      text === "Scrape" && node instanceof HTMLElement && node.classList.contains("action-label"),
  );
  const main = scrapeLabel.closest(".action-row")?.querySelector(".action-row-main");
  if (!main) throw new Error("missing scrape row main button");
  await user.click(main);

  expect(onEnterMode).toHaveBeenCalledTimes(1);
  expect(onEnterMode.mock.calls[0][0].subcommand).toBe("scrape");
  expect(onSubmit).not.toHaveBeenCalled();
});

it("submits free text through Ask instead of entering argument mode", async () => {
  const user = userEvent.setup();
  const [ask] = ACTIONS.filter((action): action is PaletteAction => action.subcommand === "ask");
  if (!ask) throw new Error("missing ask action");

  function AskHarness() {
    const [selected, setSelected] = useState(0);
    return (
      <ActionList
        filtered={[ask]}
        selected={selected}
        setSelected={setSelected}
        parsed={parseCommand("why is qdrant slow today")}
        onSubmit={onSubmit}
        onEnterMode={onEnterMode}
        onHelp={onHelp}
      />
    );
  }

  render(<AskHarness />);
  await user.click(screen.getByRole("option", { name: /Ask/ }));

  expect(onSubmit).toHaveBeenCalledTimes(1);
  expect(onSubmit.mock.calls[0][0].subcommand).toBe("ask");
  expect(onSubmit.mock.calls[0][1]).toBe("why is qdrant slow today");
  expect(onEnterMode).not.toHaveBeenCalled();
});
