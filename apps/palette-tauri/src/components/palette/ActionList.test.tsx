// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, expect, it, vi } from "vitest";

import { ActionList } from "@/components/palette/ActionList";
import { ACTIONS } from "@/lib/actions";
import { parseCommand } from "@/lib/paletteView";

HTMLElement.prototype.scrollIntoView = vi.fn();

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

it("opens selected-row help without submitting or entering action mode", () => {
  render(<Harness />);
  fireEvent.click(screen.getByRole("button", { name: "Help for Help" }));
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
  fireEvent.click(screen.getByRole("button", { name: "Help for Scrape URL" }));

  expect(onHelp).toHaveBeenCalledTimes(1);
  expect(onHelp.mock.calls[0]?.[0].subcommand).toBe("scrape");
  expect(onSubmit).not.toHaveBeenCalled();
  expect(onEnterMode).not.toHaveBeenCalled();
});
