import { describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("./invoke", () => ({ invoke: invokeMock }));

import { createAiEditFlow } from "./aiEditFlow";
import { createPane } from "./filesModel";

describe("AI edit proposal ownership", () => {
  it("never reads or writes the selected file when the proposal belongs to another path", async () => {
    const pane = createPane("left");
    pane.selected = { name: "b.md", path: "b.md", isDir: false, size: 1 };
    pane.file = {
      kind: "loaded",
      value: { path: "b.md", content: "B", size: 1 },
    };
    pane.proposal = {
      forPath: "a.md",
      proposedContent: "replacement for A",
      diff: [],
      capturedModifiedUnix: null,
    };
    const dispatch = vi.fn();
    const flow = createAiEditFlow({
      panes: [pane],
      dispatch,
      client: null,
      config: null,
    });

    await flow.approveProposal("left");

    expect(invokeMock).not.toHaveBeenCalled();
    expect(dispatch).toHaveBeenCalledWith(
      expect.objectContaining({
        type: "pane/proposalApproveError",
        pane: "left",
      }),
    );
  });

  it("tags late approval failures with the originating file generation", async () => {
    const pane = createPane("left");
    pane.selected = { name: "a.md", path: "a.md", isDir: false, size: 1 };
    pane.file = {
      kind: "loaded",
      value: { path: "a.md", content: "A", size: 1 },
    };
    pane.loadGen = 4;
    pane.proposal = {
      forPath: "a.md",
      proposedContent: "new A",
      diff: [],
      capturedModifiedUnix: null,
    };
    invokeMock.mockRejectedValueOnce(new Error("read failed"));
    const dispatch = vi.fn();

    await createAiEditFlow({
      panes: [pane],
      dispatch,
      client: null,
      config: null,
    }).approveProposal("left");

    expect(dispatch).toHaveBeenLastCalledWith({
      type: "pane/proposalApproveError",
      pane: "left",
      message: "read failed",
      path: "a.md",
      loadGen: 4,
    });
  });
});
