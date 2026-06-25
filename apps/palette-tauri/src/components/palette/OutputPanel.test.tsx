// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { OutputPanel } from "./OutputPanel";
import { ACTIONS, type PaletteAction } from "@/lib/actions";
import type { RunState } from "@/lib/runState";

function action(subcommand: string): PaletteAction {
  const found = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
  if (!found) throw new Error(`missing action ${subcommand}`);
  return found;
}

const handlers = {
  onCopy: () => {},
  onRetry: () => {},
  onFollowUp: () => {},
  onHistory: () => {},
  onCollapse: () => {},
  onTogglePin: () => {},
};

function renderPanel(run: RunState, overrides: Partial<Parameters<typeof OutputPanel>[0]> = {}) {
  return render(
    <OutputPanel
      active={action("ask")}
      copied={false}
      outputKind="markdown"
      run={run}
      pinned={false}
      {...handlers}
      {...overrides}
    />,
  );
}

describe("OutputPanel run-state transitions (T-H2)", () => {
  // Synthetic stream: re-render with a growing buffer, then a terminal success,
  // mirroring how useActionRunner produces new RunState objects per delta.
  it("announces streaming then complete via the polite live region (A11Y-C2)", () => {
    const streaming: RunState = {
      kind: "streaming",
      title: "Ask",
      subtitle: "thinking",
      text: "Partial",
      outputKind: "markdown",
      requestId: "r1",
      path: "/v1/ask",
      actionLabel: "Ask",
      prompt: "hi",
    };
    const { rerender } = renderPanel(streaming);

    // The live region is terse and does NOT echo the streamed body text.
    const liveRegion = document.querySelector('[aria-live="polite"]');
    expect(liveRegion).not.toBeNull();
    expect(liveRegion?.textContent ?? "").toMatch(/streaming/i);
    expect(liveRegion?.textContent ?? "").not.toContain("Partial");

    const success: RunState = {
      kind: "success",
      title: "Ask",
      subtitle: "done",
      text: "Final answer body.",
      outputKind: "markdown",
      result: { ok: true, status: 200, method: "POST", path: "/v1/ask", payload: {} },
      prompt: "hi",
    };
    rerender(
      <OutputPanel active={action("ask")} copied={false} outputKind="markdown" run={success} pinned={false} {...handlers} />,
    );
    expect(document.querySelector('[aria-live="polite"]')?.textContent ?? "").toMatch(/complete/i);
  });

  it("renders growing streamed text without crashing on repeated deltas", () => {
    const base = {
      kind: "streaming" as const,
      title: "Ask",
      subtitle: "thinking",
      outputKind: "markdown" as const,
      requestId: "r1",
      path: "/v1/ask",
      actionLabel: "Ask",
      prompt: "q",
    };
    const { rerender } = renderPanel({ ...base, text: "a" });
    for (const text of ["a", "ab", "abc see https://example.com/page more"]) {
      rerender(
        <OutputPanel active={action("ask")} copied={false} outputKind="markdown" run={{ ...base, text }} pinned={false} {...handlers} />,
      );
    }
    // Streaming ask renders through the conversation thread.
    expect(screen.getByText(/abc see/)).toBeInTheDocument();
  });

  it("keeps completed ask output in the conversation layout", () => {
    const success: RunState = {
      kind: "success",
      title: "Ask completed",
      subtitle: "POST /v1/ask/stream",
      text: "A skill is a reusable instruction pack for an agent.",
      outputKind: "markdown",
      result: { ok: true, status: 0, method: "POST", path: "/v1/ask/stream", payload: {} },
      prompt: "what is a skill?",
    };

    renderPanel(success);

    expect(screen.getByText("You")).toBeInTheDocument();
    expect(screen.getByText("Axon")).toBeInTheDocument();
    expect(screen.getByText("what is a skill?")).toBeInTheDocument();
    expect(screen.getByText(/A skill is a reusable instruction pack/)).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Ask a follow-up" })).toBeEnabled();
    expect(document.querySelector(".ask-answer-prose")).not.toBeNull();
    expect(document.querySelector(".ask-answer pre.output-body.output-code")).toBeNull();
    expect(screen.queryByText("Question")).not.toBeInTheDocument();
  });

  it("renders completed ask transcripts with quiet chrome and collapsible sources", () => {
    const success = {
      kind: "success",
      title: "Ask completed",
      subtitle: "POST /v1/ask/stream",
      text: "Second answer.",
      outputKind: "markdown",
      result: { ok: true, status: 0, method: "POST", path: "/v1/ask/stream", payload: {} },
      prompt: "second question",
      transcript: [
        { id: "u1", role: "user", content: "first question" },
        { id: "a1", role: "assistant", content: "First answer." },
        { id: "u2", role: "user", content: "second question" },
        {
          id: "a2",
          role: "assistant",
          content: "Second answer.",
          sources: [{ label: "docs.rs", url: "https://docs.rs" }],
        },
      ],
    } satisfies RunState;

    renderPanel(success);

    expect(screen.getByText("first question")).toBeInTheDocument();
    expect(screen.getByText("First answer.")).toBeInTheDocument();
    expect(screen.getByText("second question")).toBeInTheDocument();
    expect(screen.getByText("Second answer.")).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Ask conversation" })).toBeInTheDocument();
    expect(screen.getByText("Sources")).toBeInTheDocument();
    expect(screen.getByText("docs.rs")).toBeInTheDocument();
    expect(screen.queryByText("complete")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Copy output" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "More actions" })).toBeInTheDocument();
  });
});

describe("OutputPanel copy affordance (T-L1)", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("shows the copied state while the parent's flash flag is set then reverts", () => {
    // Use `summarize` + outputKind "code": this routes to the plain <pre> body, NOT
    // the structured view or the lazy MarkdownBody, so nothing suspends. fireEvent is
    // synchronous (no userEvent pointer-delay deadlock under fake timers). The copy
    // button renders for any terminal "text" run.
    const summarize = action("summarize");
    const success: RunState = {
      kind: "success",
      title: "Summarize",
      subtitle: "done",
      text: "Plain body text",
      outputKind: "code",
      result: { ok: true, status: 200, method: "POST", path: "/v1/summarize", payload: {} },
    };
    const onCopy = vi.fn();
    // Model the App-owned 1200ms flash: parent sets copied=true on copy, clears it
    // after the timeout. We drive `copied` via a controlled re-render and advance the
    // fake clock to prove the affordance reverts only after the flash window elapses.
    const { rerender } = renderPanel(success, { active: summarize, outputKind: "code", copied: false, onCopy });

    fireEvent.click(screen.getByRole("button", { name: "Copy output" }));
    expect(onCopy).toHaveBeenCalledWith("Plain body text");

    rerender(
      <OutputPanel active={summarize} copied={true} outputKind="code" run={success} pinned={false} {...handlers} onCopy={onCopy} />,
    );
    expect(screen.getByRole("button", { name: "Copied output" })).toBeInTheDocument();

    // Mid-flash (before 1200ms) the copied state is still showing.
    vi.advanceTimersByTime(1199);
    expect(screen.getByRole("button", { name: "Copied output" })).toBeInTheDocument();

    // After the 1200ms flash window the parent clears the flag.
    vi.advanceTimersByTime(1);
    rerender(
      <OutputPanel active={summarize} copied={false} outputKind="code" run={success} pinned={false} {...handlers} onCopy={onCopy} />,
    );
    expect(screen.getByRole("button", { name: "Copy output" })).toBeInTheDocument();
  });
});
