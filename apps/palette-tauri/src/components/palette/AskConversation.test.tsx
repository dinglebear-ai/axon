// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { AskConversation } from "./AskConversation";
import type { AskTurn } from "@/lib/runState";

const noop = () => {};

function setScrollMetrics(element: HTMLElement, metrics: { scrollHeight: number; clientHeight: number; scrollTop: number }) {
  Object.defineProperty(element, "scrollHeight", { configurable: true, value: metrics.scrollHeight });
  Object.defineProperty(element, "clientHeight", { configurable: true, value: metrics.clientHeight });
  Object.defineProperty(element, "scrollTop", { configurable: true, writable: true, value: metrics.scrollTop });
}

describe("AskConversation", () => {
  it("auto-scrolls while pinned but preserves manual scrollback", () => {
    const first: AskTurn[] = [
      { id: "u1", role: "user", content: "question" },
      { id: "a1", role: "assistant", content: "partial", pending: true },
    ];
    const { rerender } = render(<AskConversation transcript={first} pending onFollowUp={noop} />);
    const thread = screen.getByRole("group", { name: "Ask conversation" });
    setScrollMetrics(thread, { scrollHeight: 1000, clientHeight: 200, scrollTop: 800 });

    rerender(
      <AskConversation
        transcript={[first[0], { ...first[1], content: "partial answer grows" }]}
        pending
        onFollowUp={noop}
      />,
    );
    expect(thread.scrollTop).toBe(1000);

    thread.scrollTop = 100;
    fireEvent.scroll(thread);
    rerender(
      <AskConversation
        transcript={[first[0], { ...first[1], content: "partial answer grows again" }]}
        pending
        onFollowUp={noop}
      />,
    );
    expect(thread.scrollTop).toBe(100);
  });

  it("renders real agent activity and the palette send icon button", () => {
    render(
      <AskConversation
        transcript={[
          { id: "u1", role: "user", content: "question" },
          {
            id: "a1",
            role: "assistant",
            content: "",
            pending: true,
            activities: [
              { id: "act1", kind: "thinking", label: "Thinking", detail: "Planning retrieval" },
              { id: "act2", kind: "tool", label: "Retrieving context", detail: "Querying collection axon" },
            ],
          },
        ]}
        onFollowUp={noop}
      />,
    );

    expect(screen.getByLabelText("Agent activity")).toBeInTheDocument();
    expect(screen.getByText("Thinking")).toBeInTheDocument();
    expect(screen.getByText("Retrieving context")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send follow-up" })).toHaveClass("command-submit");
    expect(screen.queryByRole("button", { name: "Send" })).not.toBeInTheDocument();
  });

  it("opens a slash command palette and runs selected no-input actions", () => {
    const onRunAction = vi.fn();
    render(<AskConversation answer="answer" onFollowUp={noop} onRunAction={onRunAction} />);

    const input = screen.getByRole("textbox", { name: "Ask a follow-up" });
    fireEvent.change(input, { target: { value: "/status" } });

    expect(screen.getByRole("listbox", { name: "Palette commands" })).toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("option", { name: /Status/i })[0]);

    expect(onRunAction).toHaveBeenCalledWith("status", "");
  });

  it("runs slash commands with arguments from the composer", () => {
    const onRunAction = vi.fn();
    render(<AskConversation answer="answer" onFollowUp={noop} onRunAction={onRunAction} />);

    const input = screen.getByRole("textbox", { name: "Ask a follow-up" });
    fireEvent.change(input, { target: { value: "/scrape https://example.com" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(onRunAction).toHaveBeenCalledWith("scrape", "https://example.com");
  });

  it("selects slash commands into an action chip with Tab", () => {
    const onRunAction = vi.fn();
    render(<AskConversation answer="answer" onFollowUp={noop} onRunAction={onRunAction} />);

    const input = screen.getByRole("textbox", { name: "Ask a follow-up" });
    fireEvent.change(input, { target: { value: "/scr" } });
    fireEvent.keyDown(input, { key: "Tab" });

    expect(screen.queryByRole("listbox", { name: "Palette commands" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clear Scrape selection" })).toBeInTheDocument();
    expect(input).toHaveValue("");
    expect(onRunAction).not.toHaveBeenCalled();
  });

  it("runs the selected slash command chip with the prompt input", () => {
    const onRunAction = vi.fn();
    render(<AskConversation answer="answer" onFollowUp={noop} onRunAction={onRunAction} />);

    const input = screen.getByRole("textbox", { name: "Ask a follow-up" });
    fireEvent.change(input, { target: { value: "/scrape" } });
    fireEvent.click(screen.getByRole("option", { name: /Scrape/i }));
    fireEvent.change(input, { target: { value: "https://example.com" } });
    fireEvent.submit(input.closest("form")!);

    expect(onRunAction).toHaveBeenCalledWith("scrape", "https://example.com");
  });

  it("shows chat message suggestions from indexed docs", async () => {
    const onSuggestMessage = vi.fn().mockResolvedValue([
      {
        rank: 1,
        title: "Claude Code hooks",
        url: "https://docs.example/hooks",
        snippet: "Hooks run commands around Claude Code lifecycle events.",
        score: 0.92,
      },
    ]);
    render(
      <AskConversation
        transcript={[
          { id: "u1", role: "user", content: "how do hooks work?" },
          { id: "a1", role: "assistant", content: "Hooks run at configured lifecycle points." },
        ]}
        onFollowUp={noop}
        suggestionsEnabled
        onSuggestMessage={onSuggestMessage}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Suggest docs for user message" }));

    expect(onSuggestMessage).toHaveBeenCalledWith("how do hooks work?");
    expect(await screen.findByText("Claude Code hooks")).toBeInTheDocument();
    expect(screen.getByText("https://docs.example/hooks")).toBeInTheDocument();
    expect(screen.getByText(/Hooks run commands/)).toBeInTheDocument();
  });
});
